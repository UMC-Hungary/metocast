use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::Mutex,
};

use tauri::Manager;
use tracing_subscriber::EnvFilter;

const LOG_DIR_NAME: &str = "logs";
const LOG_FILE_NAME: &str = "metocast.log";
const MAX_READ_BYTES: u64 = 2 * 1024 * 1024;

pub fn application_log_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let app_data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    Ok(app_data_dir.join(LOG_DIR_NAME).join(LOG_FILE_NAME))
}

pub fn ensure_application_log(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let path = application_log_path(app)?;
    ensure_log_file(&path)?;
    Ok(path)
}

pub fn init_application_logging(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let path = ensure_application_log(app)?;
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| format!("Failed to open application log: {e}"))?;

    tracing_subscriber::fmt()
        .with_env_filter(env_filter())
        .with_writer(Mutex::new(file))
        .with_ansi(false)
        .try_init()
        .map_err(|e| format!("Failed to initialize application logging: {e}"))?;

    Ok(path)
}

pub fn init_fallback_logging() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(env_filter())
        .try_init();
}

pub fn read_application_log(app: &tauri::AppHandle) -> Result<String, String> {
    let path = ensure_application_log(app)?;
    let mut file = File::open(&path).map_err(|e| format!("Failed to open application log: {e}"))?;
    let len = file
        .metadata()
        .map_err(|e| format!("Failed to inspect application log: {e}"))?
        .len();

    if len <= MAX_READ_BYTES {
        let mut content = String::new();
        file.read_to_string(&mut content)
            .map_err(|e| format!("Failed to read application log: {e}"))?;
        return Ok(content);
    }

    file.seek(SeekFrom::Start(len - MAX_READ_BYTES))
        .map_err(|e| format!("Failed to seek application log: {e}"))?;

    let mut buffer = Vec::with_capacity(MAX_READ_BYTES as usize);
    file.read_to_end(&mut buffer)
        .map_err(|e| format!("Failed to read application log: {e}"))?;
    let content = String::from_utf8_lossy(&buffer);
    let trimmed = content
        .split_once('\n')
        .map(|(_, rest)| rest)
        .unwrap_or(content.as_ref());

    Ok(format!(
        "[Showing the last {} MB of the application log]\n{}",
        MAX_READ_BYTES / 1024 / 1024,
        trimmed
    ))
}

pub fn copy_application_log(app: &tauri::AppHandle, destination: &Path) -> Result<(), String> {
    let path = ensure_application_log(app)?;
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create log download directory: {e}"))?;
    }

    fs::copy(&path, destination).map_err(|e| format!("Failed to download application log: {e}"))?;
    Ok(())
}

pub fn clear_application_log(app: &tauri::AppHandle) -> Result<(), String> {
    let path = ensure_application_log(app)?;
    let mut file = OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(&path)
        .map_err(|e| format!("Failed to remove application log: {e}"))?;

    writeln!(
        file,
        "=== Metocast application log cleared at {} ===",
        chrono::Utc::now().to_rfc3339()
    )
    .map_err(|e| format!("Failed to write application log header: {e}"))?;

    Ok(())
}

fn env_filter() -> EnvFilter {
    EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"))
}

fn ensure_log_file(path: &Path) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Failed to create log directory: {e}"))?;
    }

    let should_write_header = match path.metadata() {
        Ok(metadata) => metadata.len() == 0,
        Err(_) => true,
    };

    if should_write_header {
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(|e| format!("Failed to create application log: {e}"))?;
        writeln!(
            file,
            "=== Metocast application log created at {} ===",
            chrono::Utc::now().to_rfc3339()
        )
        .map_err(|e| format!("Failed to write application log header: {e}"))?;
    }

    Ok(())
}
