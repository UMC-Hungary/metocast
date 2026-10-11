//! Programs Metocast fetches on first use instead of asking anyone to install them, the way the
//! embedded PostgreSQL arrives: a pinned release, checked against its SHA-256 before it is
//! made executable, kept under the server's data directory.
use std::path::{Path, PathBuf};

use futures_util::StreamExt;
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

/// One pinned release of a single-file program.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Download {
    pub name: &'static str,
    pub version: &'static str,
    pub url: &'static str,
    pub sha256: &'static str,
}

impl Download {
    /// Where the verified program lives: `<tools>/<name>-<version>/<name>`.
    pub fn path(&self, tools_dir: &Path) -> PathBuf {
        tools_dir
            .join(format!("{}-{}", self.name, self.version))
            .join(self.name)
    }
}

/// jive-vocals (GPL-3.0, github.com/linuxmatters/jive-vocals): a static binary with FFmpeg
/// built in, so it needs nothing else on the Mac. Apple Silicon only, like the app; the other
/// hosts that compile this crate cannot run a Mach-O anyway.
pub const JIVE_VOCALS: Download = Download {
    name: "jive-vocals",
    version: "0.7.0",
    url: "https://github.com/linuxmatters/jive-vocals/releases/download/0.7.0/jive-vocals-darwin-arm64",
    sha256: "f611ac7e23ca18891492de7f89f633933b5114f2514ff1f5f34bff3390e1a941",
};

/// The verified program, downloading it first when this host doesn't have it yet.
pub async fn ensure(download: &Download, tools_dir: &Path) -> Result<PathBuf, String> {
    let path = download.path(tools_dir);
    if path.is_file() {
        return Ok(path);
    }
    let folder = path.parent().unwrap_or(tools_dir);
    tokio::fs::create_dir_all(folder)
        .await
        .map_err(|e| format!("cannot create {}: {e}", folder.display()))?;
    tracing::info!(
        name = download.name,
        version = download.version,
        url = download.url,
        "media: downloading"
    );
    let partial = path.with_extension("download");
    let result = fetch(download, &partial).await;
    if let Err(error) = result {
        let _ = tokio::fs::remove_file(&partial).await;
        return Err(error);
    }
    make_executable(&partial)?;
    tokio::fs::rename(&partial, &path)
        .await
        .map_err(|e| format!("cannot move {} into place: {e}", download.name))?;
    tracing::info!(path = %path.display(), "media: downloaded {}", download.name);
    Ok(path)
}

async fn fetch(download: &Download, partial: &Path) -> Result<(), String> {
    let failed = |e: &dyn std::fmt::Display| format!("downloading {} failed: {e}", download.name);
    let response = reqwest::get(download.url)
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|e| failed(&e.without_url()))?;
    let mut file = tokio::fs::File::create(partial)
        .await
        .map_err(|e| failed(&e))?;
    let mut hasher = Sha256::new();
    let mut body = response.bytes_stream();
    while let Some(chunk) = body.next().await {
        let chunk = chunk.map_err(|e| failed(&e.without_url()))?;
        hasher.update(&chunk);
        file.write_all(&chunk).await.map_err(|e| failed(&e))?;
    }
    file.flush().await.map_err(|e| failed(&e))?;
    let digest = hex::encode(hasher.finalize());
    if digest != download.sha256 {
        return Err(format!(
            "the downloaded {} did not match its checksum (got {digest})",
            download.name
        ));
    }
    Ok(())
}

#[cfg(unix)]
fn make_executable(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
        .map_err(|e| format!("cannot make {} executable: {e}", path.display()))
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_download_lives_in_a_versioned_folder() {
        assert_eq!(
            JIVE_VOCALS.path(Path::new("/data/tools")),
            PathBuf::from("/data/tools/jive-vocals-0.7.0/jive-vocals")
        );
        assert_eq!(JIVE_VOCALS.sha256.len(), 64);
    }

    #[tokio::test]
    async fn an_existing_copy_is_used_without_downloading() {
        let tools = std::env::temp_dir().join(format!("metocast-tools-{}", uuid::Uuid::new_v4()));
        let path = JIVE_VOCALS.path(&tools);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"").unwrap();
        assert_eq!(ensure(&JIVE_VOCALS, &tools).await.unwrap(), path);
        std::fs::remove_dir_all(&tools).unwrap();
    }
}
