use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[tauri::command]
pub fn save_bruno_collection(dir: String, files: HashMap<String, String>) -> Result<(), String> {
    let base = Path::new(&dir);
    for (rel_path, content) in files {
        let target = base.join(&rel_path);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        fs::write(&target, content).map_err(|e| e.to_string())?;
    }
    Ok(())
}
