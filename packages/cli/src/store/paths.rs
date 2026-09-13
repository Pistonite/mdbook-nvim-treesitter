//! Filesystem facts that differ between platforms.

use std::path::{Path, PathBuf};

#[allow(unused)] // windows
use cu::pre::*;

/// The current user's home directory.
///
/// The environment variable is preferred over the standard library's lookup so
/// that a shell which has deliberately moved `HOME` is respected.
pub fn home_dir() -> Option<PathBuf> {
    let explicit = if cfg!(windows) {
        std::env::var_os("USERPROFILE")
    } else {
        std::env::var_os("HOME")
    };
    explicit
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            #[allow(deprecated)]
            std::env::home_dir()
        })
}

/// The file name an executable takes on this platform.
pub fn executable_name(stem: &str) -> String {
    if cfg!(windows) {
        format!("{stem}.exe")
    } else {
        stem.to_string()
    }
}

/// Mark a downloaded file executable.
///
/// A no-op on Windows, where executability comes from the extension rather
/// than a permission bit.
#[cfg(unix)]
pub fn make_executable(path: &Path) -> cu::Result<()> {
    use std::os::unix::fs::PermissionsExt as _;
    let permissions = std::fs::Permissions::from_mode(0o755);
    cu::check!(
        std::fs::set_permissions(path, permissions),
        "failed to make {} executable",
        path.display()
    )
}

#[cfg(not(unix))]
pub fn make_executable(_path: &Path) -> cu::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn executable_names_follow_the_platform() {
        let name = executable_name("tree-sitter");
        if cfg!(windows) {
            assert_eq!(name, "tree-sitter.exe");
        } else {
            assert_eq!(name, "tree-sitter");
        }
    }

    #[test]
    fn the_home_directory_is_findable_in_a_normal_environment() {
        assert!(home_dir().is_some());
    }
}
