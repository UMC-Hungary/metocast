//! Display-free core bootstrap shared by the Tauri desktop app and the headless
//! server binary: embedded PostgreSQL → migrations → pool → connectors → Axum.

use crate::HostHandle;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;

use crate::connectors::{
    ConnectorConfig, ConnectorStatus, FacebookConfig, YouTubeConfig, atem::AtemConnector,
    blackmagic_camera::BlackmagicCameraConnector, broadlink::BroadlinkConnector,
    facebook::FacebookConnector, middlecontrol::MiddlecontrolConnector, obs::ObsConnector,
    rodecaster::RodecasterConnector, vmix::VmixConnector, youtube::YouTubeConnector,
};
use crate::{database, scheduler::CronScheduler, server};

/// Everything the core needs to boot. `new()` fills in stand-alone defaults;
/// the desktop app overwrites the fields it shares with `AppRuntime`.
pub struct CoreOptions {
    pub data_dir: PathBuf,
    pub port: u16,
    pub auth_token: Arc<RwLock<String>>,
    /// Directory served for unmatched routes. Falls back to host assets when absent.
    pub static_dir: Option<String>,
    /// `None` in headless mode — desktop events/assets/logs are unavailable.
    pub host: Option<HostHandle>,
    /// Grants read access to stored upstream credentials. Regenerated every run
    /// and never persisted; the desktop app receives it over Tauri IPC, which no
    /// remote client can reach. `METOCAST_ADMIN_TOKEN` overrides it for headless
    /// operators and tests.
    pub admin_token: Arc<String>,
    pub obs_connector: Arc<ObsConnector>,
    pub blackmagic_camera_connector: Arc<BlackmagicCameraConnector>,
    pub middlecontrol_connector: Arc<MiddlecontrolConnector>,
    pub atem_connector: Arc<AtemConnector>,
    pub rodecaster_connector: Arc<RodecasterConnector>,
    pub vmix_connector: Arc<VmixConnector>,
    pub youtube_connector: Arc<YouTubeConnector>,
    pub facebook_connector: Arc<FacebookConnector>,
    pub broadlink_connector: Arc<BroadlinkConnector>,
    pub youtube_config: Arc<RwLock<YouTubeConfig>>,
    pub facebook_config: Arc<RwLock<FacebookConfig>>,
    pub oauth_states: Arc<RwLock<HashMap<String, (String, Instant)>>>,
    /// Legacy desktop-store configs to import into `app_settings` on first boot.
    pub seed_configs: Vec<(String, serde_json::Value)>,
    #[cfg(target_os = "macos")]
    pub keynote_connector: Arc<crate::connectors::keynote::KeynoteConnector>,
}

impl CoreOptions {
    pub fn new(data_dir: PathBuf, port: u16, auth_token: Arc<RwLock<String>>) -> Self {
        Self {
            data_dir,
            port,
            auth_token,
            static_dir: default_static_dir(),
            host: None,
            admin_token: Arc::new(
                std::env::var("METOCAST_ADMIN_TOKEN")
                    .unwrap_or_else(|_| uuid::Uuid::new_v4().to_string()),
            ),
            obs_connector: Arc::new(ObsConnector::new()),
            blackmagic_camera_connector: Arc::new(BlackmagicCameraConnector::new()),
            middlecontrol_connector: Arc::new(MiddlecontrolConnector::new()),
            atem_connector: Arc::new(AtemConnector::new()),
            rodecaster_connector: Arc::new(RodecasterConnector::new()),
            vmix_connector: Arc::new(VmixConnector::new()),
            youtube_connector: Arc::new(YouTubeConnector::new()),
            facebook_connector: Arc::new(FacebookConnector::new()),
            broadlink_connector: Arc::new(BroadlinkConnector::new()),
            youtube_config: Arc::new(RwLock::new(YouTubeConfig::default())),
            facebook_config: Arc::new(RwLock::new(FacebookConfig::default())),
            oauth_states: Arc::new(RwLock::new(HashMap::new())),
            seed_configs: Vec::new(),
            #[cfg(target_os = "macos")]
            keynote_connector: Arc::new(crate::connectors::keynote::KeynoteConnector::new()),
        }
    }
}

/// `METOCAST_STATIC_DIR` wins; otherwise dev builds serve the sibling `build/`
/// directory produced by `pnpm build`. Release builds embed the frontend in the
/// executable and serve it through the Tauri asset resolver instead.
fn default_static_dir() -> Option<String> {
    if let Ok(dir) = std::env::var("METOCAST_STATIC_DIR") {
        return Some(dir);
    }
    #[cfg(debug_assertions)]
    {
        let build_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()?
            .parent()?
            .join("build");
        build_dir
            .is_dir()
            .then(|| build_dir.to_string_lossy().into_owned())
    }
    #[cfg(not(debug_assertions))]
    {
        None
    }
}

/// Boots the full stack and blocks until the server stops.
pub async fn start(options: CoreOptions) -> anyhow::Result<()> {
    run(options, None, None).await
}

/// Boots the full stack in the background and returns explicit shutdown control.
pub async fn spawn(options: CoreOptions) -> anyhow::Result<crate::ServerHandle> {
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
    let task = tokio::spawn(async move {
        run(options, Some(shutdown_rx), Some(ready_tx))
            .await
            .map_err(|error| error.to_string())
    });
    match ready_rx.await {
        Ok(Ok(address)) => Ok(crate::ServerHandle::new(address, shutdown_tx, task)),
        Ok(Err(error)) => {
            let _ = task.await;
            Err(anyhow::anyhow!(error))
        }
        Err(_) => {
            let error = task
                .await
                .map_err(|join_error| anyhow::anyhow!(join_error))?
                .err()
                .unwrap_or_else(|| "server stopped before binding".to_string());
            Err(anyhow::anyhow!(error))
        }
    }
}

async fn run(
    options: CoreOptions,
    shutdown: Option<tokio::sync::watch::Receiver<bool>>,
    ready: Option<tokio::sync::oneshot::Sender<Result<std::net::SocketAddr, String>>>,
) -> anyhow::Result<()> {
    tracing::info!("Starting embedded PostgreSQL in {:?}", options.data_dir);
    let embedded = database::embedded::EmbeddedDb::start(options.data_dir).await?;
    let connection_url = embedded.connection_url.clone();

    tracing::info!("Connecting pool to embedded PostgreSQL");
    let pool = database::create_pool(&connection_url).await?;

    tracing::info!("Running migrations");
    database::run_migrations(&pool).await?;

    // One-time import of configs the desktop app used to keep in its Tauri store.
    for (key, value) in &options.seed_configs {
        database::settings::seed_json(&pool, key, value).await?;
    }

    // Connector configs live in `app_settings` so headless runs read the same
    // values the UI writes over HTTP.
    *options.youtube_config.write().await =
        database::settings::get_json(&pool, "youtube_config").await;
    *options.facebook_config.write().await =
        database::settings::get_json(&pool, "facebook_config").await;

    // Auto-start connectors now that the pool is available. They check for stored
    // tokens/config and stay Disconnected if none exist.
    let obs_cfg: crate::connectors::ObsConfig =
        database::settings::get_json(&pool, "obs_config").await;
    if obs_cfg.is_configured() {
        options
            .obs_connector
            .start(obs_cfg, options.host.clone())
            .await;
    }
    let camera_cfg: crate::connectors::BlackmagicCameraConfig =
        database::settings::get_json(&pool, "blackmagic_camera_config").await;
    if camera_cfg.is_configured() {
        options
            .blackmagic_camera_connector
            .start(camera_cfg, options.host.clone())
            .await;
    }
    let middlecontrol_cfg: crate::connectors::MiddlecontrolConfig =
        database::settings::get_json(&pool, "middlecontrol_config").await;
    if middlecontrol_cfg.is_configured() {
        options
            .middlecontrol_connector
            .start(middlecontrol_cfg)
            .await;
    }
    let atem_cfg: crate::connectors::AtemConfig =
        database::settings::get_json(&pool, "atem_config").await;
    options.atem_connector.start(atem_cfg).await;
    let rodecaster_cfg: crate::connectors::RodecasterConfig =
        database::settings::get_json(&pool, "rodecaster_config").await;
    if rodecaster_cfg.is_configured() {
        options.rodecaster_connector.start(rodecaster_cfg);
    }
    // Started whenever enabled, not only when fully set up: the connector itself
    // reports a missing credential or login as an error the UI can raise.
    let yt_cfg = options.youtube_config.read().await.clone();
    if yt_cfg.enabled {
        options
            .youtube_connector
            .start(pool.clone(), yt_cfg, options.host.clone())
            .await;
    }
    let fb_cfg = options.facebook_config.read().await.clone();
    if fb_cfg.enabled {
        options
            .facebook_connector
            .start(pool.clone(), fb_cfg, options.host.clone())
            .await;
    }

    // Initialise Broadlink status from DB: Connected if at least one device exists.
    let device_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM broadlink_devices")
        .fetch_one(&pool)
        .await
        .unwrap_or(0);
    if device_count > 0 {
        options
            .broadlink_connector
            .set_status(ConnectorStatus::Connected)
            .await;
    }

    tracing::info!("Starting Axum on port {}", options.port);
    server::build_and_serve(server::ServerOptions {
        pool,
        auth_token: options.auth_token,
        connection_url,
        port: options.port,
        static_dir: options.static_dir,
        obs_connector: options.obs_connector,
        blackmagic_camera_connector: options.blackmagic_camera_connector,
        middlecontrol_connector: options.middlecontrol_connector,
        atem_connector: options.atem_connector,
        rodecaster_connector: options.rodecaster_connector,
        vmix_connector: options.vmix_connector,
        youtube_connector: options.youtube_connector,
        facebook_connector: options.facebook_connector,
        broadlink_connector: options.broadlink_connector,
        youtube_config: options.youtube_config,
        facebook_config: options.facebook_config,
        oauth_states: options.oauth_states,
        host: options.host,
        admin_token: options.admin_token,
        cron_scheduler: Arc::new(CronScheduler::new()),
        shutdown,
        ready,
        #[cfg(target_os = "macos")]
        keynote_connector: options.keynote_connector,
    })
    .await?;

    embedded.stop().await?;

    Ok(())
}
