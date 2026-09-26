use serde::{Deserialize, Serialize};
use std::ffi::OsStr;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::{RwLock, broadcast};

/// Closes whatever Keynote has open, opens the file in `argv`, and plays it.
const OPEN_AND_PLAY: &str = r#"on run argv
  set presentationFile to POSIX file (item 1 of argv)
  tell application "Keynote"
    close every document saving no
    open presentationFile
    delay 1
    start slideshow of document 1
  end tell
end run"#;

/// File types Keynote can open as a presentation.
const PRESENTATION_EXTENSIONS: [&str; 3] = ["key", "pptx", "ppt"];

/// Accepts an absolute path to an existing presentation. A `.key` document may be a
/// package directory, so existence is checked rather than "is a regular file".
fn presentation_path(path: &str) -> Result<&Path, String> {
    let path = Path::new(path);
    let is_presentation = path.extension().and_then(OsStr::to_str).is_some_and(|ext| {
        PRESENTATION_EXTENSIONS
            .iter()
            .any(|known| ext.eq_ignore_ascii_case(known))
    });
    if !path.is_absolute() || !is_presentation {
        return Err(format!("Not a presentation file: {}", path.display()));
    }
    if !path.exists() {
        return Err(format!("No such file: {}", path.display()));
    }
    Ok(path)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct KeynoteStatus {
    pub app_running: bool,
    pub slideshow_active: bool,
    pub current_slide: Option<u32>,
    pub total_slides: Option<u32>,
    pub document_name: Option<String>,
}

pub struct KeynoteConnector {
    status: Arc<RwLock<KeynoteStatus>>,
    pub status_tx: broadcast::Sender<KeynoteStatus>,
}

impl KeynoteConnector {
    pub fn new() -> Self {
        let (status_tx, _) = broadcast::channel(16);
        Self {
            status: Arc::new(RwLock::new(KeynoteStatus::default())),
            status_tx,
        }
    }

    /// Returns Ok if Keynote.app is present on this system, Err otherwise.
    pub async fn check_installed() -> Result<(), String> {
        let output = tokio::process::Command::new("osascript")
            .arg("-e")
            .arg(r#"POSIX path of (path to application "Keynote")"#)
            .output()
            .await
            .map_err(|e| format!("osascript unavailable: {e}"))?;
        if output.status.success() {
            Ok(())
        } else {
            Err("Keynote is not installed on this system".to_string())
        }
    }

    async fn run_applescript(script: &str) -> Result<String, String> {
        Self::run_applescript_with_args(script, &[]).await
    }

    /// Runs `script` with `args` passed to its `on run argv` handler. Arguments stay
    /// data: nothing in them is ever parsed as AppleScript.
    async fn run_applescript_with_args(script: &str, args: &[&OsStr]) -> Result<String, String> {
        let output = tokio::process::Command::new("osascript")
            .arg("-e")
            .arg(script)
            .args(args)
            .output()
            .await
            .map_err(|e| e.to_string())?;
        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
        } else {
            Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
        }
    }

    /// Opens `path` in Keynote and starts the slideshow. The path reaches AppleScript
    /// as an argument, never as script text, so no file name can run code.
    pub async fn open_file(&self, path: &str) -> Result<(), String> {
        let path = presentation_path(path)?;
        Self::run_applescript_with_args(OPEN_AND_PLAY, &[path.as_os_str()]).await?;
        let status = self.poll_status().await;
        self.update_status(status).await;
        Ok(())
    }

    pub async fn next(&self) -> Result<(), String> {
        Self::run_applescript(r#"tell application "Keynote" to show next"#).await?;
        Ok(())
    }

    pub async fn prev(&self) -> Result<(), String> {
        Self::run_applescript(r#"tell application "Keynote" to show previous"#).await?;
        Ok(())
    }

    pub async fn first(&self) -> Result<(), String> {
        Self::run_applescript(r#"tell application "Keynote" to show slide 1 of document 1"#)
            .await?;
        Ok(())
    }

    pub async fn last(&self) -> Result<(), String> {
        Self::run_applescript(r#"tell application "Keynote" to show last slide of document 1"#)
            .await?;
        Ok(())
    }

    pub async fn goto(&self, slide: u32) -> Result<(), String> {
        let script = format!(r#"tell application "Keynote" to show slide {slide} of document 1"#);
        Self::run_applescript(&script).await?;
        Ok(())
    }

    pub async fn start_slideshow(&self) -> Result<(), String> {
        Self::run_applescript(r#"tell application "Keynote" to start slideshow of document 1"#)
            .await?;
        let status = self.poll_status().await;
        self.update_status(status).await;
        Ok(())
    }

    pub async fn stop_slideshow(&self) -> Result<(), String> {
        Self::run_applescript(r#"tell application "Keynote" to stop slideshow of document 1"#)
            .await?;
        let status = self.poll_status().await;
        self.update_status(status).await;
        Ok(())
    }

    pub async fn close_all(&self) -> Result<(), String> {
        Self::run_applescript(r#"tell application "Keynote" to close every document saving no"#)
            .await?;
        let status = self.poll_status().await;
        self.update_status(status).await;
        Ok(())
    }

    pub async fn get_status(&self) -> KeynoteStatus {
        self.status.read().await.clone()
    }

    async fn poll_status(&self) -> KeynoteStatus {
        // `playing` is an application-level property.
        // `current slide` is a document property.
        // `slide number` is read inside a `tell curSlide` block to avoid the
        // ambiguity between the `slide` class name and the `slide number` property.
        let script = r#"tell application "Keynote"
  if (count of documents) is 0 then
    return "false|false|0|0|"
  end if
  set isRunning to playing
  tell document 1
    set slideTotal to count slides
    set docTitle to name
    if isRunning then
      set curSlide to current slide
      tell curSlide
        set slideNum to slide number
      end tell
    else
      set slideNum to 0
    end if
  end tell
  return "true|" & (isRunning as string) & "|" & (slideNum as string) & "|" & (slideTotal as string) & "|" & docTitle
end tell"#;

        match Self::run_applescript(script).await {
            Ok(output) => {
                let parts: Vec<&str> = output.splitn(5, '|').collect();
                if parts.len() >= 5 {
                    let app_running = parts[0] == "true";
                    let slideshow_active = parts[1] == "true";
                    let current_slide = parts[2].parse::<u32>().ok().filter(|&n| n > 0);
                    let total_slides = parts[3].parse::<u32>().ok().filter(|&n| n > 0);
                    let document_name = if parts[4].is_empty() {
                        None
                    } else {
                        Some(parts[4].to_string())
                    };
                    KeynoteStatus {
                        app_running,
                        slideshow_active,
                        current_slide,
                        total_slides,
                        document_name,
                    }
                } else {
                    KeynoteStatus::default()
                }
            }
            Err(_) => KeynoteStatus::default(),
        }
    }

    async fn update_status(&self, new_status: KeynoteStatus) {
        let mut current = self.status.write().await;
        if *current != new_status {
            *current = new_status.clone();
            let _ = self.status_tx.send(new_status);
        }
    }

    pub fn start_polling(self: Arc<Self>) {
        tokio::spawn(async move {
            if let Err(e) = Self::check_installed().await {
                tracing::error!("Keynote connector disabled: {e}");
                return;
            }
            loop {
                let status = self.poll_status().await;
                let is_active = status.slideshow_active;
                self.update_status(status).await;
                let delay = if is_active {
                    tokio::time::Duration::from_secs(1)
                } else {
                    tokio::time::Duration::from_secs(5)
                };
                tokio::time::sleep(delay).await;
            }
        });
    }
}

impl Default for KeynoteConnector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presentation_path_accepts_only_existing_presentations() {
        let deck =
            std::env::temp_dir().join(format!("metocast-keynote-{}.KEY", std::process::id()));
        std::fs::write(&deck, b"").unwrap();

        assert_eq!(
            presentation_path(deck.to_str().unwrap()),
            Ok(deck.as_path())
        );
        assert!(
            presentation_path("Decks/Song.key").is_err(),
            "relative path"
        );
        assert!(
            presentation_path("/etc/passwd").is_err(),
            "not a presentation"
        );
        assert!(
            presentation_path("/no/such/Song.pptx").is_err(),
            "missing file"
        );

        std::fs::remove_file(&deck).ok();
    }

    /// A file name that closes the old string literal and appends a command comes
    /// back unchanged, so it was never compiled as AppleScript.
    #[tokio::test]
    async fn applescript_arguments_are_not_code() {
        let hostile = "/tmp/Song\" \ndo shell script \"echo injected\"\n--.key";
        let echoed = KeynoteConnector::run_applescript_with_args(
            "on run argv\n  return item 1 of argv\nend run",
            &[OsStr::new(hostile)],
        )
        .await
        .unwrap();

        assert_eq!(echoed, hostile);
    }
}
