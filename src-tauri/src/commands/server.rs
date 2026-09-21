use std::sync::Arc;
use tauri::State;
use tokio::sync::RwLock;

use crate::AppRuntime;

#[tauri::command]
pub async fn get_server_port(runtime: State<'_, Arc<RwLock<AppRuntime>>>) -> Result<u16, String> {
    let rt = runtime.read().await;
    Ok(rt.server_port)
}

/// Server mode needs the embedded PostgreSQL + Axum stack; mobile is client-only.
#[tauri::command]
pub fn can_host_server() -> bool {
    cfg!(desktop)
}

#[tauri::command]
pub async fn get_app_mode(
    runtime: State<'_, Arc<RwLock<AppRuntime>>>,
) -> Result<Option<String>, String> {
    let rt = runtime.read().await;
    Ok(rt.mode.clone())
}

#[tauri::command]
pub async fn set_app_mode(
    mode: String,
    url: Option<String>,
    runtime: State<'_, Arc<RwLock<AppRuntime>>>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let mut rt = runtime.write().await;
    rt.mode = Some(mode.clone());
    if let Some(u) = url {
        rt.client_url = Some(u.clone());
        save_setting(&app, "server_url", &u).await?;
    }
    save_setting(&app, "mode", &mode).await?;
    Ok(())
}

#[tauri::command]
pub async fn complete_setup(
    mode: String,
    server_url: Option<String>,
    client_token: Option<String>,
    runtime: State<'_, Arc<RwLock<AppRuntime>>>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    if mode != "server" && mode != "client" {
        return Err(format!("Invalid mode: {mode}"));
    }

    // Server mode requires desktop (embedded PostgreSQL + Axum).
    #[cfg(mobile)]
    if mode == "server" {
        return Err("Server mode is not supported on mobile devices".to_string());
    }

    save_setting(&app, "mode", &mode).await?;

    if mode == "client" {
        let url = server_url
            .as_deref()
            .filter(|s| !s.is_empty())
            .ok_or("server_url is required for client mode")?;
        let token = client_token
            .as_deref()
            .filter(|s| !s.is_empty())
            .ok_or("client_token is required for client mode")?;

        save_setting(&app, "server_url", url).await?;
        save_setting(&app, "client_auth_token", token).await?;

        let mut rt = runtime.write().await;
        rt.client_url = Some(url.to_string());
        *rt.auth_token.write().await = token.to_string();
        rt.mode = Some(mode);
    } else {
        // server mode — desktop only
        #[cfg(desktop)]
        {
            runtime.write().await.mode = Some(mode);

            let runtime_clone = Arc::clone(&runtime);
            let handle = app.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = crate::host::start_from_app_runtime(&runtime_clone, handle).await {
                    tracing::error!("Backend startup failed: {e}");
                }
            });
        }
    }

    Ok(())
}

/// Hands the admin token to the host's own webview. IPC is in-process, so no
/// remote client can reach this. Only valid while this app is the server —
/// in client mode the secrets live on someone else's machine.
#[cfg(desktop)]
#[tauri::command]
pub async fn get_admin_token(
    runtime: State<'_, Arc<RwLock<AppRuntime>>>,
) -> Result<String, String> {
    let rt = runtime.read().await;
    if rt.mode.as_deref() != Some("server") {
        return Err("Admin token is only available in server mode".to_string());
    }
    Ok(rt.admin_token.to_string())
}

#[tauri::command]
pub async fn get_client_url(
    runtime: State<'_, Arc<RwLock<AppRuntime>>>,
) -> Result<Option<String>, String> {
    let rt = runtime.read().await;
    Ok(rt.client_url.clone())
}

#[tauri::command]
pub async fn get_client_token(
    runtime: State<'_, Arc<RwLock<AppRuntime>>>,
) -> Result<String, String> {
    let rt = runtime.read().await;
    let token = rt.auth_token.read().await.clone();
    Ok(token)
}

#[tauri::command]
pub async fn reset_setup(
    runtime: State<'_, Arc<RwLock<AppRuntime>>>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    use tauri_plugin_store::StoreExt;
    let store = app.store("app-settings.json").map_err(|e| e.to_string())?;
    store.delete("mode");
    store.delete("server_url");
    store.delete("client_auth_token");
    store.save().map_err(|e| e.to_string())?;

    {
        let mut rt = runtime.write().await;
        rt.mode = None;
        rt.client_url = None;
        rt.auth_token.write().await.clear();
    }

    // The embedded PostgreSQL + Axum stack starts once per process and has no
    // teardown; setting up again in the same process kills its own PG and dies.
    #[cfg(desktop)]
    app.restart();

    #[cfg(mobile)]
    Ok(())
}

#[tauri::command]
pub fn get_local_host() -> Option<String> {
    mdns_hostname().or_else(local_ip)
}

/// The Bonjour/mDNS name this machine advertises on the LAN, e.g. `amacmini.local`.
/// Preferred over the raw IP because it survives DHCP lease changes.
#[cfg(target_os = "macos")]
fn mdns_hostname() -> Option<String> {
    let out = std::process::Command::new("scutil")
        .args(["--get", "LocalHostName"])
        .output()
        .ok()?;
    let name = String::from_utf8(out.stdout).ok()?.trim().to_string();
    if name.is_empty() {
        None
    } else {
        Some(format!("{name}.local"))
    }
}

#[cfg(not(target_os = "macos"))]
fn mdns_hostname() -> Option<String> {
    None
}

fn local_ip() -> Option<String> {
    use std::net::UdpSocket;
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    Some(socket.local_addr().ok()?.ip().to_string())
}

async fn save_setting(app: &tauri::AppHandle, key: &str, value: &str) -> Result<(), String> {
    use tauri_plugin_store::StoreExt;
    let store = app.store("app-settings.json").map_err(|e| e.to_string())?;
    store.set(key, serde_json::Value::String(value.to_string()));
    store.save().map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    #[test]
    fn resolves_a_reachable_mdns_hostname() {
        let name = super::mdns_hostname().expect("macOS always has a LocalHostName");
        assert!(name.ends_with(".local"), "got {name}");
        assert!(!name.starts_with('.'), "got {name}");
    }
}
