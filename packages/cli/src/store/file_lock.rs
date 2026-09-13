//! Cross-process file locking.
//!
//! Several mdBook builds can run at once -- a watch loop, CI, a second book --
//! and all of them may decide that the same parser is missing. Without a lock
//! they would clone into and compile in the same directory simultaneously.
//!
//! The lock is `std::fs::File::lock`, an advisory whole-file lock (`flock` on
//! Unix, `LockFileEx` on Windows). Using the OS primitive rather than a
//! hand-rolled lockfile is what makes crashes safe: the kernel drops the lock
//! when the owning process dies, so a killed build cannot wedge anything. The
//! owner's process id is written into the file purely so that a human looking
//! at a stalled build can tell who holds it.
//!
//! What the lock file is *called* is not this module's business -- see
//! [`SystemCache`](crate::store::SystemCache) for the `<key>.lock` convention.

use std::fs::{File, OpenOptions};
use std::io::Write as _;
use std::path::PathBuf;

use cu::pre::*;

/// An exclusive, process-wide lock on one file, released on drop.
pub struct FileLock {
    file: File,
}

impl FileLock {
    /// Take the lock on `path`, creating it if needed, blocking until it is
    /// available.
    pub fn acquire(path: impl Into<PathBuf>) -> cu::Result<Self> {
        let path = path.into();
        if let Some(parent) = path.parent() {
            cu::fs::make_dir(parent)?;
        }

        let file = cu::check!(
            OpenOptions::new()
                .create(true)
                .read(true)
                .write(true)
                .truncate(false)
                .open(&path),
            "failed to open lock file {}",
            path.display()
        )?;

        cu::check!(
            file.lock(),
            "failed to lock {} -- another build may be stuck",
            path.display()
        )?;

        // Best effort: a lock whose owner cannot be recorded is still a valid
        // lock, so a failure to write the pid must not fail the build.
        let _ = (&file).write_all(format!("{}\n", std::process::id()).as_bytes());

        Ok(Self { file })
    }
}

impl Drop for FileLock {
    fn drop(&mut self) {
        // Dropping the file would release the lock anyway; unlocking first
        // makes the intent explicit and surfaces nothing to the caller, since
        // there is no useful recovery from a failed unlock.
        let _ = self.file.unlock();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("mnt-lock-test-{name}"));
        let _ = cu::fs::rec_remove(&path);
        cu::fs::make_dir(&path).unwrap();
        path
    }

    #[test]
    fn records_the_owning_process_id() {
        let path = temp_dir("pid").join("thing.lock");
        let _lock = FileLock::acquire(&path).unwrap();
        let recorded = cu::fs::read_string(&path).unwrap();
        assert_eq!(recorded.trim(), std::process::id().to_string());
    }

    #[test]
    fn relocking_after_release_succeeds() {
        let path = temp_dir("sequential").join("thing.lock");
        drop(FileLock::acquire(&path).unwrap());
        drop(FileLock::acquire(&path).unwrap());
    }

    #[test]
    fn creates_missing_parent_directories() {
        let path = temp_dir("missing")
            .join("nested")
            .join("deeper")
            .join("thing.lock");
        let _lock = FileLock::acquire(&path).unwrap();
        assert!(path.exists());
    }
}
