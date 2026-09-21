use metocast_server::{ApplicationLog, Host, HostAsset, HostError};
use std::sync::Arc;

use tauri::{Emitter, Manager};
use tauri_plugin_store::StoreExt;
use tokio::sync::RwLock;

pub struct TauriHost(tauri::AppHandle);

impl TauriHost {
    pub fn new(handle: tauri::AppHandle) -> Self {
        Self(handle)
    }
}

impl Host for TauriHost {
    fn emit(&self, event: &str, payload: serde_json::Value) -> Result<(), HostError> {
        self.0
            .emit(event, payload)
            .map_err(|error| HostError::Failed(error.to_string()))
    }

    fn asset(&self, path: &str) -> Result<Option<HostAsset>, HostError> {
        Ok(self
            .0
            .asset_resolver()
            .get(path.to_string())
            .map(|asset| HostAsset {
                bytes: asset.bytes,
                mime_type: asset.mime_type,
            }))
    }

    fn caption_logo(&self) -> Result<Option<String>, HostError> {
        let store = self
            .0
            .store("caption-settings.json")
            .map_err(|error| HostError::Failed(error.to_string()))?;
        Ok(store
            .get("svgLogo")
            .and_then(|value| value.as_str().map(String::from)))
    }

    fn application_log(&self) -> Result<ApplicationLog, HostError> {
        let path = crate::logging::ensure_application_log(&self.0).map_err(HostError::Failed)?;
        let content = crate::logging::read_application_log(&self.0).map_err(HostError::Failed)?;
        Ok(ApplicationLog {
            path: path.to_string_lossy().into_owned(),
            content,
        })
    }

    fn clear_application_log(&self) -> Result<(), HostError> {
        crate::logging::clear_application_log(&self.0).map_err(HostError::Failed)
    }
}

const LEGACY_CONFIG_KEYS: [&str; 7] = [
    "obs_config",
    "vmix_config",
    "atem_config",
    "broadlink_config",
    "discord_config",
    "youtube_config",
    "facebook_config",
];

pub async fn start_from_app_runtime(
    app_runtime: &RwLock<crate::AppRuntime>,
    app: tauri::AppHandle,
) -> anyhow::Result<()> {
    let data_dir = app
        .path()
        .app_data_dir()
        .unwrap_or_else(|_| std::path::PathBuf::from("./data"));
    let seed_configs = app
        .store("app-settings.json")
        .map(|store| {
            LEGACY_CONFIG_KEYS
                .iter()
                .filter_map(|key| store.get(*key).map(|value| ((*key).to_string(), value)))
                .collect()
        })
        .unwrap_or_default();

    let mut options = {
        let runtime = app_runtime.read().await;
        let mut options = metocast_server::runtime::CoreOptions::new(
            data_dir,
            runtime.server_port,
            Arc::clone(&runtime.auth_token),
        );
        options.host = Some(Arc::new(TauriHost::new(app)));
        options.admin_token = Arc::clone(&runtime.admin_token);
        options.obs_connector = Arc::clone(&runtime.obs_connector);
        options.blackmagic_camera_connector = Arc::clone(&runtime.blackmagic_camera_connector);
        options.middlecontrol_connector = Arc::clone(&runtime.middlecontrol_connector);
        options.atem_connector = Arc::clone(&runtime.atem_connector);
        options.vmix_connector = Arc::clone(&runtime.vmix_connector);
        options.youtube_connector = Arc::clone(&runtime.youtube_connector);
        options.facebook_connector = Arc::clone(&runtime.facebook_connector);
        options.broadlink_connector = Arc::clone(&runtime.broadlink_connector);
        options.youtube_config = Arc::clone(&runtime.youtube_config);
        options.facebook_config = Arc::clone(&runtime.facebook_config);
        options.oauth_states = Arc::clone(&runtime.oauth_states);
        #[cfg(target_os = "macos")]
        {
            options.keynote_connector = Arc::clone(&runtime.keynote_connector);
        }
        options
    };
    options.seed_configs = seed_configs;
    metocast_server::runtime::start(options).await
}
