use serde::Serialize;
use std::path::{Path, PathBuf};
use tauri::Manager;

const PRESENTATION_FONTS: [(&str, &str); 2] = [
    (
        "CormorantGaramond-Regular.ttf",
        "CormorantGaramond-Regular.ttf",
    ),
    ("GeistMono-Regular.ttf", "GeistMono-Regular.ttf"),
];

const LEGACY_PRESENTATION_FONTS: [&str; 2] = [
    "Metocast-CormorantGaramond-Variable.ttf",
    "Metocast-GeistMono-Variable.ttf",
];

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PresentationFontsStatus {
    supported: bool,
    installed: bool,
    install_dir: Option<String>,
}

fn status(font_dir: &Path, supported: bool) -> PresentationFontsStatus {
    PresentationFontsStatus {
        supported,
        installed: supported
            && PRESENTATION_FONTS
                .iter()
                .all(|(_, destination)| font_dir.join(destination).is_file()),
        install_dir: supported.then(|| font_dir.display().to_string()),
    }
}

#[cfg(target_os = "macos")]
fn prepare_font_book(resource_dir: &Path, font_dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut sources = Vec::with_capacity(PRESENTATION_FONTS.len());
    for (source_name, _) in PRESENTATION_FONTS {
        let source = resource_dir.join("fonts").join(source_name);
        if !source.is_file() {
            return Err(format!("Bundled font is missing: {}", source.display()));
        }
        sources.push(source);
    }
    for legacy_name in LEGACY_PRESENTATION_FONTS {
        let legacy = font_dir.join(legacy_name);
        if let Err(error) = std::fs::remove_file(&legacy)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            return Err(format!(
                "Cannot remove the old font {}: {error}",
                legacy.display()
            ));
        }
    }
    Ok(sources)
}

#[tauri::command]
pub fn presentation_fonts_status(app: tauri::AppHandle) -> Result<PresentationFontsStatus, String> {
    #[cfg(target_os = "macos")]
    {
        let font_dir = app.path().font_dir().map_err(|error| error.to_string())?;
        Ok(status(&font_dir, true))
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = app;
        Ok(status(Path::new(""), false))
    }
}

#[tauri::command]
pub fn install_presentation_fonts(
    app: tauri::AppHandle,
) -> Result<PresentationFontsStatus, String> {
    #[cfg(target_os = "macos")]
    {
        let resource_dir = app
            .path()
            .resource_dir()
            .map_err(|error| error.to_string())?;
        let font_dir = app.path().font_dir().map_err(|error| error.to_string())?;
        let sources = prepare_font_book(&resource_dir, &font_dir)?;
        std::process::Command::new("/usr/bin/open")
            .arg("-a")
            .arg("Font Book")
            .args(sources)
            .spawn()
            .map_err(|error| format!("Cannot open Font Book: {error}"))?;
        Ok(status(&font_dir, true))
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = app;
        Err("Presentation font installation is supported only on macOS".to_string())
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    #[test]
    fn prepares_static_fonts_and_removes_the_old_variable_files() {
        let root =
            std::env::temp_dir().join(format!("metocast-font-install-test-{}", std::process::id()));
        let resources = root.join("resources");
        let fonts = root.join("user-fonts");
        std::fs::create_dir_all(resources.join("fonts")).unwrap();
        std::fs::create_dir_all(&fonts).unwrap();
        for legacy in LEGACY_PRESENTATION_FONTS {
            std::fs::write(fonts.join(legacy), legacy).unwrap();
        }
        for (source, _) in PRESENTATION_FONTS {
            std::fs::write(resources.join("fonts").join(source), source).unwrap();
        }

        let sources = prepare_font_book(&resources, &fonts).unwrap();

        assert_eq!(sources.len(), PRESENTATION_FONTS.len());
        assert!(sources.iter().all(|source| source.is_file()));
        assert!(
            LEGACY_PRESENTATION_FONTS
                .iter()
                .all(|legacy| !fonts.join(legacy).exists())
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
