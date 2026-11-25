//! One editor per project: the process holds an exclusive lock on a file
//! of the system's temp folder named after the project's folder, so a
//! second editor of it (another VS Code window) can't start. The system
//! lets the lock go when the process ends, however it ends: a crash
//! leaves nothing stale behind.

use std::fs::{File, OpenOptions, TryLockError};
use std::path::{Path, PathBuf};

/// The lock held: the project's editor is this process while it lives.
#[derive(Debug)]
pub struct Lock {
    _file: File,
}

impl Lock {
    /// Takes the lock of the project at `root`, or says why it can't.
    pub fn take(root: &Path) -> Result<Lock, String> {
        Self::take_in(&std::env::temp_dir(), root)
    }

    /// [`take`](Self::take), with the lock files in `dir`.
    pub fn take_in(dir: &Path, root: &Path) -> Result<Lock, String> {
        let path = path_in(dir, root);
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&path)
            .map_err(|e| format!("can't open {}: {e}", path.display()))?;
        match file.try_lock() {
            Ok(()) => Ok(Lock { _file: file }),
            Err(TryLockError::WouldBlock) => {
                Err(format!("the World Editor of {} is open in another window", root.display()))
            }
            Err(TryLockError::Error(e)) => Err(format!("can't lock {}: {e}", path.display())),
        }
    }
}

/// The lock file of the project at `root` in `dir`: by its folder,
/// normalized and in lower case (the same folder however it's written).
pub fn path_in(dir: &Path, root: &Path) -> PathBuf {
    let root = std::fs::canonicalize(root).unwrap_or_else(|_| wc3_server::project::normalize(root));
    let key = root.to_string_lossy().replace('\\', "/").to_lowercase();
    // FNV-1a: the same name in every process, whatever its build.
    let hash = key.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| (h ^ b as u64).wrapping_mul(0x0100_0000_01b3));
    dir.join(format!("wc3-editor-{hash:016x}.lock"))
}

#[cfg(test)]
#[path = "lock.test.rs"]
mod tests;
