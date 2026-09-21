#[tauri::command]
pub fn get_application_log_path(app: tauri::AppHandle) -> Result<String, String> {
    crate::logging::ensure_application_log(&app).map(|path| path.to_string_lossy().into_owned())
}

#[tauri::command]
pub fn read_application_log(app: tauri::AppHandle) -> Result<String, String> {
    crate::logging::read_application_log(&app)
}

#[tauri::command]
pub fn download_application_log(app: tauri::AppHandle, destination: String) -> Result<(), String> {
    crate::logging::copy_application_log(&app, std::path::Path::new(&destination))
}

#[tauri::command]
pub fn remove_application_log(app: tauri::AppHandle) -> Result<(), String> {
    crate::logging::clear_application_log(&app)
}

#[tauri::command]
pub fn open_application_log(app: tauri::AppHandle) -> Result<(), String> {
    let path = crate::logging::ensure_application_log(&app)?;
    match tauri_plugin_opener::open_path(&path, None::<&str>) {
        Ok(()) => Ok(()),
        Err(open_error) => {
            if tauri_plugin_opener::reveal_item_in_dir(&path).is_ok() {
                return Ok(());
            }

            if let Some(parent) = path.parent()
                && tauri_plugin_opener::open_path(parent, None::<&str>).is_ok()
            {
                return Ok(());
            }

            Err(format!("Failed to open application log: {open_error}"))
        }
    }
}
