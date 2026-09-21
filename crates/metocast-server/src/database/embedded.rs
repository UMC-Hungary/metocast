use anyhow::{Context, Result};
use postgresql_embedded::{PostgreSQL, Settings, V18};
#[cfg(target_os = "macos")]
use std::fs;
#[cfg(target_os = "macos")]
use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;
use tokio_postgres::{Client, NoTls};

const DB_NAME: &str = "metocast";
const PG_PORT: u16 = 15432;
const PG_USER: &str = "postgres";
const PG_PASS: &str = "metocast_embedded";
const PG_RELEASES_URL: &str = "https://github.com/zonkyio/embedded-postgres-binaries";
const POSTGRES_BOOTSTRAP_DB: &str = "postgres";

pub struct EmbeddedDb {
    pub pg: PostgreSQL,
    pub connection_url: String,
}

impl EmbeddedDb {
    pub async fn start(data_dir: PathBuf) -> Result<Self> {
        let installation_dir = data_dir.join("pg_install");

        #[cfg(target_os = "macos")]
        remove_incompatible_macos_postgres_installations(&installation_dir)?;

        let settings = Settings {
            releases_url: PG_RELEASES_URL.to_string(),
            version: (*V18).clone(),
            port: PG_PORT,
            installation_dir,
            data_dir: data_dir.join("pg_data"),
            temporary: false,
            username: PG_USER.to_string(),
            password: PG_PASS.to_string(),
            ..Settings::default()
        };

        tracing::info!(
            releases_url = %settings.releases_url,
            version = %settings.version,
            installation_dir = %settings.installation_dir.display(),
            data_dir = %settings.data_dir.display(),
            "Configuring embedded PostgreSQL"
        );

        let mut pg = PostgreSQL::new(settings.clone());
        if let Err(error) = setup_and_start_postgres(&mut pg).await {
            #[cfg(target_os = "macos")]
            if is_homebrew_openssl_dyld_error(&error) {
                tracing::warn!(
                    error = %error,
                    installation_dir = %settings.installation_dir.display(),
                    "Embedded PostgreSQL startup used an incompatible Homebrew-linked binary; reinstalling binaries and retrying"
                );
                remove_macos_postgres_install_root(&settings.installation_dir)?;
                pg = PostgreSQL::new(settings);
                setup_and_start_postgres(&mut pg).await?;
            } else {
                return Err(error);
            }

            #[cfg(not(target_os = "macos"))]
            return Err(error);
        }

        let connection_url = ensure_application_database(pg.settings()).await?;

        Ok(Self { pg, connection_url })
    }

    pub async fn stop(self) -> Result<()> {
        self.pg.stop().await?;
        Ok(())
    }
}

async fn setup_and_start_postgres(pg: &mut PostgreSQL) -> Result<()> {
    pg.setup().await?;

    // If start fails (stale process from a previous session left on the
    // port), kill whatever is using the port and retry once.
    if let Err(e) = pg.start().await {
        tracing::warn!(
            "PG start failed ({e}); killing stale process on port {PG_PORT} and retrying"
        );
        kill_on_port(PG_PORT).await;
        tokio::time::sleep(Duration::from_secs(2)).await;
        pg.start().await?;
    }

    Ok(())
}

async fn ensure_application_database(settings: &Settings) -> Result<String> {
    let client = connect_to_database(settings, POSTGRES_BOOTSTRAP_DB, PG_PASS).await?;

    if !database_exists(&client, DB_NAME).await? {
        tracing::info!("Creating embedded PostgreSQL database for Metocast");
        client
            .execute(&format!("CREATE DATABASE {}", quote_pg_ident(DB_NAME)), &[])
            .await
            .context("failed to create embedded PostgreSQL database")?;
    }

    Ok(database_url(settings, DB_NAME, PG_PASS))
}

async fn connect_to_database(
    settings: &Settings,
    database_name: &str,
    password: &str,
) -> Result<Client> {
    let (client, connection) =
        tokio_postgres::connect(&database_url(settings, database_name, password), NoTls)
            .await
            .with_context(|| {
                format!("failed to connect to embedded PostgreSQL database {database_name}")
            })?;

    tokio::spawn(async move {
        if let Err(error) = connection.await {
            tracing::warn!(
                %error,
                "Embedded PostgreSQL maintenance connection closed with error"
            );
        }
    });

    Ok(client)
}

async fn database_exists(client: &Client, database_name: &str) -> Result<bool> {
    let exists = client
        .query_one(
            "SELECT EXISTS(SELECT 1 FROM pg_database WHERE datname = $1)",
            &[&database_name],
        )
        .await
        .with_context(|| format!("failed to inspect embedded PostgreSQL database {database_name}"))?
        .get(0);

    Ok(exists)
}

fn database_url(settings: &Settings, database_name: &str, password: &str) -> String {
    let mut settings = settings.clone();
    settings.password = password.to_string();
    settings.url(database_name)
}

fn quote_pg_ident(identifier: &str) -> String {
    format!("\"{}\"", identifier.replace('"', "\"\""))
}

#[cfg(target_os = "macos")]
fn remove_incompatible_macos_postgres_installations(install_root: &Path) -> Result<()> {
    if !install_root.exists() {
        return Ok(());
    }

    if is_incompatible_macos_postgres_installation(install_root) {
        remove_incompatible_macos_postgres_installation(install_root)?;
        return Ok(());
    }

    let entries = fs::read_dir(install_root).with_context(|| {
        format!(
            "failed to inspect embedded PostgreSQL installation directory {}",
            install_root.display()
        )
    })?;

    for entry in entries {
        let path = entry
            .with_context(|| {
                format!(
                    "failed to read embedded PostgreSQL installation entry in {}",
                    install_root.display()
                )
            })?
            .path();

        if path.is_dir() && is_incompatible_macos_postgres_installation(&path) {
            remove_incompatible_macos_postgres_installation(&path)?;
        }
    }

    Ok(())
}

#[cfg(target_os = "macos")]
fn is_incompatible_macos_postgres_installation(installation_dir: &Path) -> bool {
    let lib_dir = installation_dir.join("lib");

    lib_dir.join("libpq.5.dylib").exists()
        && (!lib_dir.join("libssl.3.dylib").exists() || !lib_dir.join("libcrypto.3.dylib").exists())
}

#[cfg(target_os = "macos")]
fn remove_incompatible_macos_postgres_installation(installation_dir: &Path) -> Result<()> {
    tracing::warn!(
        installation_dir = %installation_dir.display(),
        "Removing incompatible embedded PostgreSQL installation; it does not bundle OpenSSL dylibs"
    );

    fs::remove_dir_all(installation_dir).with_context(|| {
        format!(
            "failed to remove incompatible embedded PostgreSQL installation {}",
            installation_dir.display()
        )
    })
}

#[cfg(target_os = "macos")]
fn is_homebrew_openssl_dyld_error(error: &anyhow::Error) -> bool {
    let message = format!("{error:#}");

    message.contains("Library not loaded: /opt/homebrew/opt/openssl@3/lib/libssl.3.dylib")
        || message.contains("Library not loaded: /opt/homebrew/opt/openssl@3/lib/libcrypto.3.dylib")
}

#[cfg(target_os = "macos")]
fn remove_macos_postgres_install_root(install_root: &Path) -> Result<()> {
    if !install_root.exists() {
        return Ok(());
    }

    tracing::warn!(
        installation_dir = %install_root.display(),
        "Removing embedded PostgreSQL installation root before reinstall"
    );

    fs::remove_dir_all(install_root).with_context(|| {
        format!(
            "failed to remove embedded PostgreSQL installation root {}",
            install_root.display()
        )
    })
}

/// Kill any processes listening on `port` using `lsof` + SIGTERM.
async fn kill_on_port(port: u16) {
    let output = match tokio::process::Command::new("lsof")
        .args(["-t", "-i", &format!("TCP:{port}")])
        .output()
        .await
    {
        Ok(o) => o,
        Err(e) => {
            tracing::warn!("lsof failed: {e}");
            return;
        }
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    for pid_str in stdout.split_whitespace() {
        if let Ok(pid) = pid_str.parse::<u32>() {
            tracing::info!("Sending SIGTERM to stale PG process {pid}");
            let _ = tokio::process::Command::new("kill")
                .args(["-TERM", &pid.to_string()])
                .status()
                .await;
        }
    }
}

#[cfg(all(test, target_os = "macos"))]
mod macos_tests {
    use super::*;
    use std::fs::File;

    #[test]
    fn removes_installations_missing_bundled_openssl() -> Result<()> {
        let install_root =
            std::env::temp_dir().join(format!("metocast-pg-install-test-{}", uuid::Uuid::new_v4()));
        let broken_lib_dir = install_root.join("18.3.0").join("lib");
        let valid_lib_dir = install_root.join("18.4.0").join("lib");

        fs::create_dir_all(&broken_lib_dir)?;
        fs::create_dir_all(&valid_lib_dir)?;
        File::create(broken_lib_dir.join("libpq.5.dylib"))?;
        File::create(valid_lib_dir.join("libpq.5.dylib"))?;
        File::create(valid_lib_dir.join("libssl.3.dylib"))?;
        File::create(valid_lib_dir.join("libcrypto.3.dylib"))?;

        remove_incompatible_macos_postgres_installations(&install_root)?;

        assert!(!install_root.join("18.3.0").exists());
        assert!(install_root.join("18.4.0").exists());

        fs::remove_dir_all(install_root)?;
        Ok(())
    }

    #[test]
    fn detects_homebrew_openssl_dyld_errors() {
        let error = anyhow::anyhow!(
            "Command error: stdout=; stderr=dyld[1828]: Library not loaded: /opt/homebrew/opt/openssl@3/lib/libssl.3.dylib"
        );

        assert!(is_homebrew_openssl_dyld_error(&error));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_postgres_identifiers() {
        assert_eq!(quote_pg_ident("metocast"), "\"metocast\"");
        assert_eq!(quote_pg_ident("meto\"cast"), "\"meto\"\"cast\"");
    }
}
