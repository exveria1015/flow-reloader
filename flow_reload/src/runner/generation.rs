use super::{LibraryCopy, RunnerError};
use libloading::Library;
use std::sync::{Arc, Mutex};
use std::thread::{self, ThreadId};

/// Determines what happens to a retired dynamic library.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum UnloadPolicy {
    /// Keep code alive until process termination. Independent tasks and callbacks
    /// cannot be discovered automatically. Copied files may remain after exit.
    #[default]
    KeepLoaded,
    /// Permit explicit collection on the owner thread after leases are gone.
    /// Both enabling this policy and collecting require unsafe contracts.
    WhenIdle,
}

// Never run plugin cleanup from a foreign Waker's drop callback. The queue
// retains an owning reference until an explicit collection on the owner thread.
static RETIRED: Mutex<Vec<RetiredGeneration>> = Mutex::new(Vec::new());

struct RetiredGeneration {
    image: Arc<LibraryGeneration>,
    owner: ThreadId,
    policy: UnloadPolicy,
}

pub(super) struct LibraryGeneration {
    // Declaration order is significant: unload code before removing its file.
    library: Library,
    _copy: LibraryCopy,
}

/// Releases retired WhenIdle generations with no remaining tracked leases.
///
/// Only libraries created on the calling thread are collected. KeepLoaded
/// generations are never collected. The return value counts released host
/// library handles; the OS may retain additional references to a module.
///
/// # Safety
/// All retired WhenIdle artifacts on this thread must be quiescent: no plugin
/// code, destructors, or callbacks may be executing or scheduled without a live
/// lease. In particular, a DLL-side Waker trampoline can still be returning
/// after its final host lease is released. Join/synchronize those callers before
/// collection. Reference counts alone do not prove that a DLL call has returned.
pub unsafe fn collect_retired_libraries() -> usize {
    let owner = thread::current().id();
    let mut ready = Vec::new();
    {
        let mut retired = RETIRED.lock().unwrap_or_else(|err| err.into_inner());
        let mut index = 0;
        while index < retired.len() {
            let entry = &retired[index];
            if entry.policy == UnloadPolicy::WhenIdle
                && entry.owner == owner
                && Arc::strong_count(&entry.image) == 1
            {
                ready.push(retired.swap_remove(index));
            } else {
                index += 1;
            }
        }
    }
    let count = ready.len();
    // Plugin termination routines may reenter host code. Do not hold the lock.
    drop(ready);
    count
}

pub(crate) struct GenerationOwner {
    image: Option<Arc<LibraryGeneration>>,
    owner: ThreadId,
    pub(super) policy: UnloadPolicy,
}

impl GenerationOwner {
    pub(crate) fn load(source: &std::path::Path, id: u64) -> Result<Self, RunnerError> {
        Self::open(super::copy_library(source, id)?, UnloadPolicy::KeepLoaded)
    }
    pub(super) fn open(copy: LibraryCopy, policy: UnloadPolicy) -> Result<Self, RunnerError> {
        let library = unsafe { Library::new(&copy.path) }.inspect_err(|_| {
            crate::retention::open_failed();
        })?;
        crate::retention::update(copy.id, |v| {
            v.loaded = true;
            v.policy = policy;
        });
        Ok(Self {
            image: Some(Arc::new(LibraryGeneration {
                library,
                _copy: copy,
            })),
            owner: thread::current().id(),
            policy,
        })
    }

    pub(crate) fn prepared(&self) {
        crate::retention::update(self.artifact_id(), |v| {
            v.state = crate::ImageState::Prepared;
            v.outcome = crate::ImageOutcome::Candidate;
        });
    }
    pub(crate) fn activate(&self) {
        crate::retention::update(self.artifact_id(), |v| {
            v.state = crate::ImageState::Active;
            v.outcome = crate::ImageOutcome::Adopted;
        });
    }
    pub(crate) fn retire(&self) {
        crate::retention::update(self.artifact_id(), |v| v.state = crate::ImageState::Retired);
    }
    pub(super) fn set_policy(&mut self, policy: UnloadPolicy) {
        self.policy = policy;
        crate::retention::update(self.artifact_id(), |v| v.policy = policy);
    }
    fn artifact_id(&self) -> crate::ArtifactId {
        self.image
            .as_ref()
            .expect("generation image missing")
            ._copy
            .id
    }

    pub(crate) fn library(&self) -> &Library {
        &self
            .image
            .as_ref()
            .expect("generation image missing")
            .library
    }

    pub(crate) fn lease(&self) -> Arc<dyn Send + Sync> {
        self.image
            .as_ref()
            .expect("generation image missing")
            .clone()
    }

    #[cfg(test)]
    pub(super) fn copy_path(&self) -> &std::path::Path {
        &self
            .image
            .as_ref()
            .expect("generation image missing")
            ._copy
            .path
    }
}

impl Drop for GenerationOwner {
    fn drop(&mut self) {
        let image = self.image.take().expect("generation image missing");
        crate::retention::update(image._copy.id, |v| {
            v.state = crate::ImageState::Retired;
            v.outcome = match v.outcome {
                crate::ImageOutcome::Pending => crate::ImageOutcome::Rejected,
                crate::ImageOutcome::Candidate => crate::ImageOutcome::Abandoned,
                other => other,
            };
        });
        RETIRED
            .lock()
            .unwrap_or_else(|err| err.into_inner())
            .push(RetiredGeneration {
                image,
                owner: self.owner,
                policy: self.policy,
            });
    }
}
