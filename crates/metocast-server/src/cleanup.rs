//! Removing a file that may already be gone, without hiding a real failure.

use std::io::ErrorKind;
use std::path::Path;

/// Deletes `path`. Not finding it is the expected case; any other failure is logged.
pub(crate) async fn remove_file_if_present(path: impl AsRef<Path>) {
    let path = path.as_ref();
    if let Err(error) = tokio::fs::remove_file(path).await
        && error.kind() != ErrorKind::NotFound
    {
        tracing::warn!(path = %path.display(), %error, "could not remove file");
    }
}
