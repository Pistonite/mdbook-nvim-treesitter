//! Cross-process file locking.
//!
//! Several mdBook builds can run at once -- a watch loop, CI, a second book --
//! and all of them may decide that the same parser is missing. Without a lock
//! they would clone into and compile in the same directory simultaneously.
//!
//! The lock is `std::fs::File::lock`, a whole-file lock (`flock` on Unix,
//! `LockFileEx` on Windows). Using the OS primitive rather than a hand-rolled
//! lockfile is what makes crashes safe: the kernel drops the lock when the
//! owning process dies, so a killed build cannot wedge anything.
//!
//! The owner's process id is written into the file as a diagnostic, so that a
//! human looking at a stalled build can tell who holds it. **That only works
//! on Unix.** Windows byte-range locks are mandatory rather than advisory, so
//! while the lock is held any other handle -- another process, or another
//! handle in this one -- gets `ERROR_LOCK_VIOLATION` trying to read the file.
//! The pid is readable there only once the lock is gone, which is exactly when
//! it stops being interesting. It is written anyway: it costs one `write_all`
//! and it is worth having on the platform where it works.
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

        // Never `truncate(true)`: opening happens before locking, so it would
        // wipe the incumbent owner's pid while they still hold the lock. The
        // file is emptied after the lock is ours instead.
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
        // lock, so a failure here must not fail the build.
        let _ = record_owner(&file);

        Ok(Self { file })
    }
}

/// Replace the file's contents with this process's id.
///
/// The truncate matters: pids vary in length, and writing a short one over a
/// longer one would leave the previous owner's trailing digits behind, so
/// `cat`ing the lock file would show a pid that never existed.
fn record_owner(file: &File) -> std::io::Result<()> {
    file.set_len(0)?;
    // The handle was just opened and nothing has moved the cursor, so this
    // writes at offset zero.
    (&mut &*file).write_all(format!("{}\n", std::process::id()).as_bytes())
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

    /// The pid, read after the lock is released.
    ///
    /// It has to be read after: on Windows the lock is mandatory, so nothing
    /// can open the file for reading while it is held -- not even this process.
    fn recorded_owner(path: &PathBuf) -> String {
        cu::fs::read_string(path).unwrap().trim().to_string()
    }

    #[test]
    fn records_the_owning_process_id() {
        let path = temp_dir("pid").join("thing.lock");
        drop(FileLock::acquire(&path).unwrap());

        assert_eq!(recorded_owner(&path), std::process::id().to_string());
    }

    #[test]
    fn a_new_owner_leaves_none_of_the_previous_ones_digits() {
        let path = temp_dir("overwrite").join("thing.lock");
        // A previous owner with a longer pid than ours. Without truncating,
        // its trailing digits would survive and the file would name a process
        // that never ran.
        cu::fs::write(&path, format!("{}\n", u32::MAX)).unwrap();

        drop(FileLock::acquire(&path).unwrap());

        assert_eq!(recorded_owner(&path), std::process::id().to_string());
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
