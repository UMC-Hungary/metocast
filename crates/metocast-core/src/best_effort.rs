//! Results that nobody can act on are never thrown away. A best-effort send, a cleanup or a
//! flush that fails is logged with the place it happened, so a failure that is not benign still
//! shows up. `let _ = ...` hides that; these do not.

use std::fmt::Display;
use std::io;
use std::panic::Location;
use std::path::Path;

/// The other side is gone: a channel whose receiver was dropped, a child that already exited.
/// Expected during shutdown and when nobody subscribed, so it is logged at `trace`.
pub trait LogGone {
    #[track_caller]
    fn log_gone(self);
}

impl<T, E> LogGone for Result<T, E> {
    #[track_caller]
    fn log_gone(self) {
        if self.is_err() {
            tracing::trace!(at = %Location::caller(), "nobody on the other side");
        }
    }
}

/// An operation that could not be completed and has no recovery here (a flush, a database
/// write, a socket send). Logged at `warn` with the error.
pub trait LogFailed {
    #[track_caller]
    fn log_failed(self);
}

impl<T, E: Display> LogFailed for Result<T, E> {
    #[track_caller]
    fn log_failed(self) {
        if let Err(error) = self {
            tracing::warn!(at = %Location::caller(), %error, "best-effort operation failed");
        }
    }
}

/// Removes a file that may already be gone. Not finding it is the expected case; any other
/// failure is logged.
#[track_caller]
pub fn remove_file_if_present(path: impl AsRef<Path>) {
    ignore_not_found(std::fs::remove_file(path.as_ref()), path.as_ref());
}

/// Removes a directory tree that may already be gone, like [`remove_file_if_present`].
#[track_caller]
pub fn remove_dir_all_if_present(path: impl AsRef<Path>) {
    ignore_not_found(std::fs::remove_dir_all(path.as_ref()), path.as_ref());
}

#[track_caller]
fn ignore_not_found(result: io::Result<()>, path: &Path) {
    match result {
        Err(error) if error.kind() != io::ErrorKind::NotFound => {
            tracing::warn!(at = %Location::caller(), path = %path.display(), %error, "cleanup failed");
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_path_is_fine_and_an_existing_one_is_removed() {
        let dir = std::env::temp_dir().join(format!("metocast-best-effort-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("inner")).unwrap();
        let file = dir.join("inner").join("partial");
        std::fs::write(&file, b"x").unwrap();

        remove_file_if_present(&file);
        assert!(!file.exists());
        remove_file_if_present(&file); // already gone: no panic, nothing to report
        remove_dir_all_if_present(&dir);
        assert!(!dir.exists());
        remove_dir_all_if_present(&dir);
    }

    #[test]
    fn results_of_either_kind_are_accepted() {
        Err::<(), _>("closed").log_gone();
        Ok::<(), &str>(()).log_gone();
        Err::<(), _>("disk full").log_failed();
        Ok::<(), &str>(()).log_failed();
    }
}
