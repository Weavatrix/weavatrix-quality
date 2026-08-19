//! Content-addressed blob store under `.weavatrix-quality/objects`.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use wvq_domain::ContentHash;

use crate::sqlite::StoreError;

/// Filesystem CAS. Large blobs never enter `SQLite`.
#[derive(Debug, Clone)]
pub struct Cas {
    root: PathBuf,
}

impl Cas {
    /// `root` is `.weavatrix-quality/objects`.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::Io`] when the directory cannot be created.
    pub fn open(root: impl AsRef<Path>) -> Result<Self, StoreError> {
        let root = root.as_ref().to_path_buf();
        fs::create_dir_all(&root).map_err(|err| StoreError::Io {
            path: root.display().to_string(),
            message: err.to_string(),
        })?;
        Ok(Self { root })
    }

    /// Write bytes if absent. Same content always yields the same hash.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::Io`] on write failure.
    pub fn put(&self, bytes: &[u8]) -> Result<ContentHash, StoreError> {
        let hash = hash_bytes(bytes)?;
        let path = self.object_path(&hash);
        if path.is_file() {
            return Ok(hash);
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|err| StoreError::Io {
                path: parent.display().to_string(),
                message: err.to_string(),
            })?;
        }
        let tmp = path.with_extension("tmp");
        {
            let mut file = fs::File::create(&tmp).map_err(|err| StoreError::Io {
                path: tmp.display().to_string(),
                message: err.to_string(),
            })?;
            file.write_all(bytes).map_err(|err| StoreError::Io {
                path: tmp.display().to_string(),
                message: err.to_string(),
            })?;
        }
        fs::rename(&tmp, &path).map_err(|err| StoreError::Io {
            path: path.display().to_string(),
            message: err.to_string(),
        })?;
        Ok(hash)
    }

    /// Read a blob.
    ///
    /// # Errors
    ///
    /// Missing objects are [`StoreError::MissingBlob`].
    pub fn get(&self, hash: &ContentHash) -> Result<Vec<u8>, StoreError> {
        let path = self.object_path(hash);
        fs::read(&path).map_err(|err| {
            if err.kind() == std::io::ErrorKind::NotFound {
                StoreError::MissingBlob(hash.to_string())
            } else {
                StoreError::Io {
                    path: path.display().to_string(),
                    message: err.to_string(),
                }
            }
        })
    }

    /// Absolute object path `ab/<fullhash>`.
    #[must_use]
    pub fn object_path(&self, hash: &ContentHash) -> PathBuf {
        let hex = hash.as_str();
        let prefix = hex.get(..2).unwrap_or("00");
        self.root.join(prefix).join(hex)
    }

    /// Count object files (for tests).
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::Io`] on walk failure.
    pub fn object_count(&self) -> Result<usize, StoreError> {
        let mut count = 0_usize;
        if !self.root.is_dir() {
            return Ok(0);
        }
        for prefix in fs::read_dir(&self.root).map_err(|err| StoreError::Io {
            path: self.root.display().to_string(),
            message: err.to_string(),
        })? {
            let prefix = prefix.map_err(|err| StoreError::Io {
                path: self.root.display().to_string(),
                message: err.to_string(),
            })?;
            if !prefix.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                continue;
            }
            for object in fs::read_dir(prefix.path()).map_err(|err| StoreError::Io {
                path: prefix.path().display().to_string(),
                message: err.to_string(),
            })? {
                let object = object.map_err(|err| StoreError::Io {
                    path: prefix.path().display().to_string(),
                    message: err.to_string(),
                })?;
                if object.path().extension().is_some_and(|ext| ext == "tmp") {
                    continue;
                }
                if object.file_type().map(|t| t.is_file()).unwrap_or(false) {
                    count = count.saturating_add(1);
                }
            }
        }
        Ok(count)
    }
}

fn hash_bytes(bytes: &[u8]) -> Result<ContentHash, StoreError> {
    let digest = Sha256::digest(bytes);
    let hex = digest.iter().fold(String::new(), |mut out, byte| {
        use std::fmt::Write as _;
        let _ = write!(out, "{byte:02x}");
        out
    });
    ContentHash::new(hex).map_err(|err| StoreError::Invalid(err.to_string()))
}
