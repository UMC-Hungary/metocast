//! POC CLI over the library. Exists to exercise a real camera by hand.

use std::time::Duration;

use blackmagic_camera::{Camera, Trust, discovery, notify};

fn usage() -> ! {
    eprintln!(
        "\
Usage: bmcam <command> [options]

Commands:
  discover [--service <type>] [--timeout <secs>] [--all]
  probe    <host>                       connect, show the cert fingerprint + product
  get      <host> <path>                any GET, e.g. /video/iso
  put      <host> <path> <json>         any PUT, e.g. /video/iso '{{\"iso\":800}}'
  record   <host> start|stop
  stream   <host> status|start|stop|available|platforms|platform
  stream   <host> dump                  every livestream setting, in one pass
  stream   <host> target <rtmp/srt-url> [--quality <name>]   stream to your own receiver
  watch    <host> <property>...         live propertyValueChanged events

Options (any command that takes a host):
  --user <u> --pass <p>       HTTP basic auth
  --fingerprint <sha256>      pin the camera's certificate (from `probe`)
  --insecure                  trust-on-first-use, no pinning (probing only)

Examples:
  bmcam probe cam.local
  bmcam get cam.local /system/product --fingerprint ab12...
  bmcam record cam.local start --user admin --pass secret --fingerprint ab12...
  bmcam watch cam.local /transports/0/record /video/iso --fingerprint ab12...
  bmcam stream http://cam.local target rtmp://192.168.0.12/live/cam"
    );
    std::process::exit(1);
}

struct Args {
    command: String,
    positional: Vec<String>,
    user: Option<String>,
    pass: Option<String>,
    fingerprint: Option<String>,
    insecure: bool,
    all: bool,
    quality: Option<String>,
    service: String,
    timeout: u64,
}

impl Args {
    fn parse() -> Self {
        let raw: Vec<String> = std::env::args().skip(1).collect();
        let command = raw.first().cloned().unwrap_or_else(|| usage());
        if command == "--help" || command == "-h" {
            usage();
        }

        let flag = |name: &str| -> Option<String> {
            raw.windows(2).find(|w| w[0] == name).map(|w| w[1].clone())
        };

        let flag_names = [
            "--user",
            "--pass",
            "--fingerprint",
            "--service",
            "--timeout",
            "--quality",
        ];
        let mut positional = Vec::new();
        let mut skip_next = false;
        for arg in raw.iter().skip(1) {
            if skip_next {
                skip_next = false;
                continue;
            }
            if flag_names.contains(&arg.as_str()) {
                skip_next = true;
            } else if !arg.starts_with("--") {
                positional.push(arg.clone());
            }
        }

        Self {
            command,
            positional,
            user: flag("--user"),
            pass: flag("--pass"),
            fingerprint: flag("--fingerprint"),
            insecure: raw.iter().any(|a| a == "--insecure"),
            all: raw.iter().any(|a| a == "--all"),
            quality: flag("--quality"),
            service: flag("--service").unwrap_or_else(|| discovery::DEFAULT_SERVICE.into()),
            timeout: flag("--timeout").and_then(|t| t.parse().ok()).unwrap_or(5),
        }
    }

    fn at(&self, index: usize) -> String {
        self.positional
            .get(index)
            .cloned()
            .unwrap_or_else(|| usage())
    }

    fn camera(&self) -> Camera {
        let host = self.at(0);
        // Nothing to pin on a plain-HTTP camera, so don't demand a fingerprint for one.
        let plain_http = host.starts_with("http://");
        let trust = match (&self.fingerprint, self.insecure || plain_http) {
            (Some(fp), _) => Trust::Pinned(fp.clone()),
            (None, true) => Trust::OnFirstUse,
            (None, false) => {
                eprintln!(
                    "Refusing to connect over HTTPS without --fingerprint.\n\
                     Run `bmcam probe {host}` first to see it, or pass --insecure.\n\
                     If the camera serves plain HTTP, address it as http://{host}"
                );
                std::process::exit(1);
            }
        };
        let auth = match (&self.user, &self.pass) {
            (Some(u), Some(p)) => Some((u.as_str(), p.as_str())),
            _ => None,
        };
        Camera::connect(&host, auth, trust).unwrap_or_else(|e| fail(e))
    }
}

fn fail(e: impl std::fmt::Display) -> ! {
    eprintln!("error: {e}");
    std::process::exit(1);
}

/// One labelled block of a dump: the body, or why it is missing. A 501 here means
/// "this camera model has no such feature", not a failure.
fn section(label: &str, result: Result<serde_json::Value, blackmagic_camera::Error>) {
    match result {
        Ok(v) => {
            println!("\n── {label} ──");
            print_json(&v);
        }
        Err(e) => println!("\n── {label} ──\n  ({e})"),
    }
}

fn print_json(value: &serde_json::Value) {
    println!(
        "{}",
        serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
    );
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    match args.command.as_str() {
        "discover" if args.all => {
            let lines = discovery::browse_raw(&args.service, Duration::from_secs(args.timeout))
                .await
                .unwrap_or_else(|e| fail(e));
            if lines.is_empty() {
                println!("Nothing at all on {} in {}s.", args.service, args.timeout);
            }
            for line in lines {
                println!("{line}");
            }
        }

        "discover" => {
            let found = discovery::browse(&args.service, Duration::from_secs(args.timeout))
                .await
                .unwrap_or_else(|e| fail(e));
            if found.is_empty() {
                println!(
                    "No cameras advertised on {} in {}s.\n\
                     \n\
                     If a camera is definitely on this network, mDNS multicast is most likely\n\
                     not reaching this process — on macOS that is the Local Network privacy gate\n\
                     (the system `dns-sd -B {}` still works, since it goes via mDNSResponder).\n\
                     Cross-check with:  bmcam discover --all --service {}\n\
                     \n\
                     Either way, adding by name or IP does not need multicast at all:\n\
                       bmcam probe <camera-name>.local",
                    args.service, args.timeout, args.service, args.service
                );
            }
            for camera in found {
                println!(
                    "{}  {} (firmware {})\n  host: {}\n  addresses: {:?}\n  id: {}",
                    camera.device_name,
                    camera.product_name,
                    camera.software_version,
                    camera.host(),
                    camera.addresses,
                    camera.unique_id,
                );
            }
        }

        "probe" => {
            // Deliberately trust-on-first-use: the point is to learn the fingerprint.
            let auth = match (&args.user, &args.pass) {
                (Some(u), Some(p)) => Some((u.as_str(), p.as_str())),
                _ => None,
            };
            let camera =
                Camera::connect(&args.at(0), auth, Trust::OnFirstUse).unwrap_or_else(|e| fail(e));
            let product = camera.product().await;
            match camera.presented_fingerprint() {
                Some(fp) => println!("fingerprint: {fp}"),
                None => println!("fingerprint: (plain http, no certificate)"),
            }
            match product {
                Ok(p) => println!(
                    "camera:      {} — {} (firmware {})",
                    p.device_name, p.product_name, p.software_version
                ),
                Err(e) => fail(e),
            }
        }

        "get" => {
            let path = args.at(1);
            let value: serde_json::Value =
                args.camera().get(&path).await.unwrap_or_else(|e| fail(e));
            print_json(&value);
        }

        "put" => {
            let (path, body) = (args.at(1), args.at(2));
            let body: serde_json::Value = serde_json::from_str(&body).unwrap_or_else(|e| fail(e));
            args.camera()
                .put(&path, &body)
                .await
                .unwrap_or_else(|e| fail(e));
            println!("ok");
        }

        "record" => {
            let camera = args.camera();
            let result = match args.at(1).as_str() {
                "start" => camera.start_recording(None).await,
                "stop" => camera.stop_recording().await,
                _ => usage(),
            };
            result.unwrap_or_else(|e| fail(e));
            println!("ok");
        }

        "stream" => {
            let camera = args.camera();
            match args.at(1).as_str() {
                "status" => {
                    let status = camera.livestream_status().await.unwrap_or_else(|e| fail(e));
                    println!("{status:?}");
                }
                "start" => {
                    camera.livestream_start().await.unwrap_or_else(|e| fail(e));
                    println!("ok");
                }
                "stop" => {
                    camera.livestream_stop().await.unwrap_or_else(|e| fail(e));
                    println!("ok");
                }
                // Everything the camera will say about livestreaming, in one pass.
                // Each endpoint is reported independently so a 501/404 on one
                // (they are model-dependent) still shows the rest.
                "dump" => {
                    for (label, path) in [
                        ("status", "/livestreams/0"),
                        ("available", "/livestreams/0/available"),
                        ("active platform", "/livestreams/0/activePlatform"),
                    ] {
                        section(label, camera.get::<serde_json::Value>(path).await);
                    }

                    match camera.livestream_platforms().await {
                        Ok(names) => {
                            println!("\n── platforms ──\n{}", names.join(", "));
                            for name in names {
                                section(
                                    &format!("platform: {name}"),
                                    camera
                                        .get::<serde_json::Value>(&format!(
                                            "/livestreams/platforms/{name}"
                                        ))
                                        .await,
                                );
                            }
                        }
                        Err(e) => println!("\n── platforms ──\n  ({e})"),
                    }

                    match camera.livestream_custom_platforms().await {
                        Ok(files) if files.is_empty() => {
                            println!("\n── custom platforms ──\n  (none)")
                        }
                        Ok(files) => {
                            for file in files {
                                // Custom platform files are XML, not JSON.
                                let body = camera
                                    .get_text(&format!("/livestreams/customPlatforms/{file}"))
                                    .await;
                                match body {
                                    Ok(xml) => {
                                        println!("\n── custom platform: {file} ──\n{xml}")
                                    }
                                    Err(e) => {
                                        println!("\n── custom platform: {file} ──\n  ({e})")
                                    }
                                }
                            }
                        }
                        Err(e) => println!("\n── custom platforms ──\n  ({e})"),
                    }
                }

                "target" => {
                    let url = args.at(2);
                    let platform = camera
                        .stream_to(&url, args.quality.as_deref())
                        .await
                        .unwrap_or_else(|e| fail(e));
                    println!(
                        "destination set: {} / {} (quality {})\nstart it with:  bmcam stream {} start",
                        platform.platform,
                        platform.url.unwrap_or_default(),
                        platform.quality,
                        args.at(0),
                    );
                }
                "available" => {
                    let a = camera
                        .livestream_available()
                        .await
                        .unwrap_or_else(|e| fail(e));
                    println!("{a:?}");
                }
                "platforms" => {
                    for p in camera
                        .livestream_platforms()
                        .await
                        .unwrap_or_else(|e| fail(e))
                    {
                        println!("{p}");
                    }
                }
                "platform" => {
                    let p = camera
                        .livestream_active_platform()
                        .await
                        .unwrap_or_else(|e| fail(e));
                    println!("{p:?}");
                }
                _ => usage(),
            }
        }

        "watch" => {
            let camera = args.camera();
            let properties: Vec<String> = args.positional[1..].to_vec();
            if properties.is_empty() {
                usage();
            }
            let trust = match &args.fingerprint {
                Some(fp) => Trust::Pinned(fp.clone()),
                None => Trust::OnFirstUse,
            };
            let mut events = notify::watch(&camera, trust, properties);
            println!("watching — ctrl-c to stop");
            while let Some(event) = events.recv().await {
                match event {
                    notify::Event::Connected => println!("[connected]"),
                    notify::Event::Disconnected(e) => println!("[disconnected] {e} — retrying"),
                    notify::Event::Changed(c) => println!("{} = {}", c.property, c.value),
                }
            }
        }

        _ => usage(),
    }
}
