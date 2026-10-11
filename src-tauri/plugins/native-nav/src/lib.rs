use serde::{Deserialize, Serialize};
#[cfg(target_os = "ios")]
use tauri::AppHandle;
#[cfg(target_os = "macos")]
use tauri::WebviewWindow;
use tauri::ipc::Channel;
use tauri::plugin::{Builder, TauriPlugin};
use tauri::{Runtime, command};

#[cfg(target_os = "macos")]
mod macos;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NavItem {
    pub label: String,
    pub symbol: String,
}

#[cfg(target_os = "ios")]
tauri::ios_plugin_binding!(init_plugin_native_nav);

#[cfg(target_os = "ios")]
struct Ios<R: Runtime>(tauri::plugin::PluginHandle<R>);

#[cfg(target_os = "ios")]
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct InstallPayload<'a> {
    items: &'a [NavItem],
    active: usize,
    on_select: &'a Channel<usize>,
}

#[cfg(target_os = "ios")]
#[derive(Serialize)]
struct ActivePayload {
    active: usize,
}

/// Installs the platform's own navigation surface: an `NSToolbar` selection group
/// on macOS, a `UIGlassEffect` tab bar on iOS. Returns where it sits — `top`,
/// `bottom`, or nothing at all when the platform has no native surface.
///
/// One definition per platform, each taking only what it uses.
#[cfg(target_os = "macos")]
#[command]
async fn install<R: Runtime>(
    window: WebviewWindow<R>,
    items: Vec<NavItem>,
    active: usize,
    on_select: Channel<usize>,
) -> Result<Option<&'static str>, String> {
    macos::install(&window, items, active, on_select);
    Ok(Some("top"))
}

#[cfg(target_os = "ios")]
#[command]
async fn install<R: Runtime>(
    app: AppHandle<R>,
    items: Vec<NavItem>,
    active: usize,
    on_select: Channel<usize>,
) -> Result<Option<&'static str>, String> {
    let state = <AppHandle<R> as tauri::Manager<R>>::state::<Ios<R>>(&app);
    // A rejection means this iOS is too old for the native bar; the UI keeps its own.
    match state.0.run_mobile_plugin::<()>(
        "install",
        InstallPayload {
            items: &items,
            active,
            on_select: &on_select,
        },
    ) {
        Ok(()) => Ok(Some("bottom")),
        Err(e) => {
            tracing::warn!(error = %e, "native-nav iOS plugin refused the bar");
            Ok(None)
        }
    }
}

#[cfg(not(any(target_os = "macos", target_os = "ios")))]
#[command]
async fn install() -> Result<Option<&'static str>, String> {
    Ok(None)
}

#[cfg(target_os = "macos")]
#[command]
async fn set_active<R: Runtime>(window: WebviewWindow<R>, active: usize) -> Result<(), String> {
    macos::set_active(&window, active);
    Ok(())
}

#[cfg(target_os = "ios")]
#[command]
async fn set_active<R: Runtime>(app: AppHandle<R>, active: usize) -> Result<(), String> {
    let state = <AppHandle<R> as tauri::Manager<R>>::state::<Ios<R>>(&app);
    if let Err(e) = state
        .0
        .run_mobile_plugin::<()>("setActive", ActivePayload { active })
    {
        tracing::warn!(error = %e, "native-nav iOS plugin could not set the active tab");
    }
    Ok(())
}

#[cfg(not(any(target_os = "macos", target_os = "ios")))]
#[command]
async fn set_active() -> Result<(), String> {
    Ok(())
}

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("native-nav")
        .invoke_handler(tauri::generate_handler![install, set_active])
        .setup(|_app, _api| {
            #[cfg(target_os = "ios")]
            {
                use tauri::Manager;
                let handle = _api.register_ios_plugin(init_plugin_native_nav)?;
                _app.manage(Ios(handle));
                tracing::info!("native-nav iOS plugin registered");
            }
            Ok(())
        })
        .build()
}
