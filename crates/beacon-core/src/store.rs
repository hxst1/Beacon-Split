use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::error::{CoreError, Result};

/// A single JSON document on disk.
///
/// Reads fall back to `Default` when the file does not exist yet, so a fresh
/// install needs no bootstrapping step. Writes go through a temporary file and
/// a rename so a crash mid-write cannot leave a truncated config behind.
#[derive(Debug, Clone)]
pub struct JsonStore {
    path: PathBuf,
}

impl JsonStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn read<T: DeserializeOwned + Default>(&self) -> Result<T> {
        let bytes = match fs::read(&self.path) {
            Ok(bytes) => bytes,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(T::default()),
            Err(err) => return Err(CoreError::io(&self.path, err)),
        };

        serde_json::from_slice(&bytes).map_err(|source| CoreError::Parse {
            path: self.path.clone(),
            source,
        })
    }

    pub fn write<T: Serialize>(&self, value: &T) -> Result<()> {
        let json = serde_json::to_vec_pretty(value).map_err(|source| CoreError::Serialize {
            path: self.path.clone(),
            source,
        })?;

        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|err| CoreError::io(parent, err))?;
        }

        let tmp = self.path.with_extension("json.tmp");
        fs::write(&tmp, &json).map_err(|err| CoreError::io(&tmp, err))?;
        fs::rename(&tmp, &self.path).map_err(|err| CoreError::io(&self.path, err))?;
        Ok(())
    }
}

/// Guards against silently misreading a document written by a future build.
pub fn ensure_schema(path: &Path, found: u32, supported: u32) -> Result<()> {
    if found > supported {
        return Err(CoreError::UnsupportedSchema {
            path: path.to_path_buf(),
            found,
            supported,
        });
    }
    Ok(())
}
