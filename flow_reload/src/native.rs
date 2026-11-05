//! Native images are retained until process termination, including rejected loads.
use crate::generation::GenerationOwner;
use crate::{HandlerApi, HandlerHeader, RouteError, HANDLER_ABI_VERSION};
use interface::services::HandlerApiFn;
use libloading::Library;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

#[derive(Debug)]
pub enum LoadError {
    Io(std::io::Error),
    LibLoading(libloading::Error),
}
impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "io error: {error}"),
            Self::LibLoading(error) => write!(f, "library load error: {error}"),
        }
    }
}
impl std::error::Error for LoadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::LibLoading(error) => Some(error),
        }
    }
}
impl From<std::io::Error> for LoadError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}
impl From<libloading::Error> for LoadError {
    fn from(error: libloading::Error) -> Self {
        Self::LibLoading(error)
    }
}

// Keep an owning reference from the moment Library::new succeeds. A panic or
// failed validation must never unload callbacks that may have escaped loading.
// Static storage deliberately has no destructor at process shutdown.
static IMAGES: Mutex<Vec<Arc<LibraryImage>>> = Mutex::new(Vec::new());
struct LibraryImage {
    library: Library,
    _copy: LibraryCopy,
}

struct LibraryCopy {
    path: PathBuf,
    id: crate::ArtifactId,
}

impl Drop for LibraryCopy {
    fn drop(&mut self) {
        let error = std::fs::remove_file(&self.path)
            .err()
            .filter(|e| e.kind() != std::io::ErrorKind::NotFound)
            .map(|e| crate::CopyRemovalError {
                id: self.id,
                path: self.path.clone(),
                message: e.to_string(),
            });
        crate::retention::removed(self.id, error);
    }
}

fn copy_library(source_path: &Path) -> Result<LibraryCopy, LoadError> {
    let result = (|| {
        let directory = crate::cache::session_directory()?.join("copies");
        let id = crate::retention::next_id()?;
        let ext = source_path
            .extension()
            .and_then(|s| s.to_str())
            .filter(|s| matches!(*s, "dll" | "so" | "dylib"))
            .unwrap_or("bin");
        let path = directory.join(format!("{:016x}.{ext}", id.0));
        let mut source = std::fs::File::open(source_path)?;
        let mut dest = crate::cache::new_file(&path)?;
        crate::retention::copied(id, source_path.to_path_buf(), path.clone(), 0);
        let copy = LibraryCopy { path, id };
        let bytes = std::io::copy(&mut source, &mut dest)?;
        crate::retention::update(id, |v| v.copy_bytes = bytes);
        drop(dest);
        Ok(copy)
    })();
    if result.is_err() {
        crate::retention::copy_failed();
    }
    result
}
pub(crate) unsafe fn load(path: &Path) -> Result<(HandlerApi, GenerationOwner), RouteError> {
    let copy = copy_library(path).map_err(RouteError::Load)?;
    let library = Library::new(&copy.path)
        .inspect_err(|_| {
            crate::retention::open_failed();
        })
        .map_err(|error| RouteError::Load(LoadError::LibLoading(error)))?;
    crate::retention::update(copy.id, |v| v.loaded = true);
    let id = copy.id;
    let library = Arc::new(LibraryImage {
        library,
        _copy: copy,
    });
    IMAGES
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .push(library.clone());
    let image = GenerationOwner {
        code: library.clone(),
        id,
    };
    let entry = library
        .library
        .get::<HandlerApiFn>(b"hot_handler_api")
        .map_err(|error| RouteError::Load(LoadError::LibLoading(error)))?;
    let pointer = entry();
    if pointer.is_null() {
        return Err(RouteError::InvalidApi);
    }
    // Read only the stable header before accessing the rest of the table.
    let header = pointer.cast::<HandlerHeader>().read();
    if header.abi_version != HANDLER_ABI_VERSION
        || header.struct_size < std::mem::size_of::<HandlerApi>() as u32
    {
        return Err(RouteError::Abi {
            version: header.abi_version,
            size: header.struct_size,
        });
    }
    let api = pointer.read();
    crate::routing::validate(api)?;
    Ok((api, image))
}
