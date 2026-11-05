//! Value-only diagnostics for this host SDK instance. No library leases are held here.
use crate::UnloadPolicy;
use std::collections::{BTreeMap, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ArtifactId(pub u64);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageState {
    Preparing,
    Prepared,
    Active,
    Retired,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageOutcome {
    Pending,
    Candidate,
    Adopted,
    Rejected,
    Abandoned,
}
#[derive(Clone, Debug)]
pub struct ImageRecord {
    pub id: ArtifactId,
    pub source: PathBuf,
    pub copy: PathBuf,
    pub copy_bytes: u64,
    pub loaded: bool,
    pub state: ImageState,
    pub outcome: ImageOutcome,
    pub policy: UnloadPolicy,
}
#[derive(Clone, Debug)]
pub struct CopyRemovalError {
    pub id: ArtifactId,
    pub path: PathBuf,
    pub message: String,
}
#[derive(Clone, Debug)]
pub struct RetentionSnapshot {
    pub tracked_native_images: usize,
    /// File lengths at copy completion, for images with a live SDK library handle.
    pub recorded_copy_bytes: u64,
    pub images: Vec<ImageRecord>,
    pub released_images: u64,
    pub open_failures: u64,
    pub copy_failures: u64,
    /// The latest 32 failures; the cumulative counter is not truncated.
    pub removal_errors: Vec<CopyRemovalError>,
    pub removal_error_count: u64,
}
struct Registry {
    images: BTreeMap<ArtifactId, ImageRecord>,
    released: u64,
    open_failed: u64,
    copy_failed: u64,
    errors: VecDeque<CopyRemovalError>,
    error_count: u64,
}
static REGISTRY: Mutex<Registry> = Mutex::new(Registry {
    images: BTreeMap::new(),
    released: 0,
    open_failed: 0,
    copy_failed: 0,
    errors: VecDeque::new(),
    error_count: 0,
});
static NEXT: AtomicU64 = AtomicU64::new(1);
fn registry() -> MutexGuard<'static, Registry> {
    REGISTRY.lock().unwrap_or_else(|e| e.into_inner())
}
/// Reports this SDK instance, not other SDKs/processes, OS module residency or RAM.
pub fn retention_snapshot() -> RetentionSnapshot {
    let r = registry();
    let images: Vec<_> = r.images.values().cloned().collect();
    RetentionSnapshot {
        tracked_native_images: images.iter().filter(|v| v.loaded).count(),
        recorded_copy_bytes: images
            .iter()
            .filter(|v| v.loaded)
            .fold(0u64, |n, v| n.saturating_add(v.copy_bytes)),
        images,
        released_images: r.released,
        open_failures: r.open_failed,
        copy_failures: r.copy_failed,
        removal_errors: r.errors.iter().cloned().collect(),
        removal_error_count: r.error_count,
    }
}
pub(crate) fn next_id() -> std::io::Result<ArtifactId> {
    NEXT.try_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
        .map(ArtifactId)
        .map_err(|_| std::io::Error::other("artifact IDs exhausted"))
}
pub(crate) fn copied(id: ArtifactId, source: PathBuf, copy: PathBuf, bytes: u64) {
    registry().images.insert(
        id,
        ImageRecord {
            id,
            source,
            copy,
            copy_bytes: bytes,
            loaded: false,
            state: ImageState::Preparing,
            outcome: ImageOutcome::Pending,
            policy: UnloadPolicy::KeepLoaded,
        },
    );
}
pub(crate) fn copy_failed() {
    let mut r = registry();
    r.copy_failed = r.copy_failed.saturating_add(1);
}
pub(crate) fn open_failed() {
    let mut r = registry();
    r.open_failed = r.open_failed.saturating_add(1);
}
pub(crate) fn update(id: ArtifactId, f: impl FnOnce(&mut ImageRecord)) {
    if let Some(v) = registry().images.get_mut(&id) {
        f(v);
    }
}
pub(crate) fn removed(id: ArtifactId, error: Option<CopyRemovalError>) {
    let mut r = registry();
    if let Some(v) = r.images.remove(&id) {
        if v.loaded {
            r.released = r.released.saturating_add(1);
        }
    }
    if let Some(error) = error {
        r.error_count = r.error_count.saturating_add(1);
        if r.errors.len() == 32 {
            r.errors.pop_front();
        }
        r.errors.push_back(error);
    }
}
