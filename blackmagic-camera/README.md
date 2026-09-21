# blackmagic-camera

Blackmagic Camera Control API client — REST + notification websocket, with self-signed
certificate pinning. Standalone crate: `bmcam` exercises it by hand today, Metocast embeds
the library later. This doc is both the findings from building it and the CLI reference.

## Official documentation

- [REST API for Blackmagic Cameras (PDF, 07 Aug 2025)](https://documents.blackmagicdesign.com/DeveloperManuals/RESTAPIforBlackmagicCameras.pdf) — the endpoint reference everything below is checked against; still Blackmagic's current published version, checked live against their developer page
- [Blackmagic Camera Control (PDF, 07 Aug 2025)](https://documents.blackmagicdesign.com/DeveloperManuals/BlackmagicCameraControl.pdf)
- [Developer page — Camera SDK and Software](https://www.blackmagicdesign.com/developer/products/camera/sdk-and-software) — source of truth for which PDF is current; re-check here before assuming a doc is stale
- **Per camera:** `https://<camera-host>/control/documentation.html` — the YAML docs generated from _your specific unit's installed firmware_, served by the camera itself
- Also on disk: `/Applications/Blackmagic Cameras/Instruction Manuals/Cinema Camera 6K Manual.pdf` (installed with Blackmagic Camera Setup) — "Developer Information" section, p.182 onward, is the same REST reference

**Doc/firmware gap:** the test camera is running software 10.2.2 (10 Aug 2026), which added
Blackmagic Cloud stream routing, 10-minute pre-record, and 4-channel audio — a year of camera
releases past the Aug 2025 PDF above. Blackmagic hasn't republished the REST API PDF since, so
those newer features may not appear in it. For anything camera-recent — especially the
Cloud/stream-routing surface — **prefer the camera's own `/control/documentation.html` over the
PDF**; it reflects the firmware actually installed. (There is a January 2026 "Blackmagic
Streaming REST API" doc, but it's for the Streaming Encoder HD/4K hardware, a different
product — not a newer version of this one.)

## Build

```bash
cd blackmagic-camera && cargo build && cargo test
```

Run via `cargo run --bin bmcam --` or build once and use `./target/debug/bmcam` directly.

## Before anything works: enable the API on the camera

**The REST API is off by default.** Connect the camera over USB-C, open **Blackmagic Camera
Setup** → **Network Access** → enable **Web Media Manager**. Per the manual:

> "REST API also uses HTTP and this means enabling access to media via the web media manager
> also enables camera control via REST API."

Until this is on, the camera answers mDNS and ARP but accepts no TCP connection at all — every
call here fails with `Unreachable`, and that failure mode is indistinguishable from "wrong
host" or "camera off". Check this first if nothing responds.

| Camera Setup option                              | Effect on this crate                                                                           |
| ------------------------------------------------ | ---------------------------------------------------------------------------------------------- |
| Web media manager → _enabled_                    | Plain HTTP — address the camera as `http://cam.local`                                          |
| Web media manager → _enabled with security only_ | HTTPS, self-signed cert by default — needs `--fingerprint` (or `--insecure` once, to learn it) |
| Secure login settings                            | Username + password — pass `--user`/`--pass`                                                   |
| Allow utility administration → _via USB_         | Blocks Camera Setup itself over the network; does not affect the REST API                      |

Renaming the camera or a factory reset invalidates its certificate, so its fingerprint
changes and it correctly comes back as `CertUntrusted` until re-accepted.

## CLI reference

```
bmcam <command> [options]
```

| Command                                                             | Does                                                                                                   |
| ------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------ |
| `discover [--service <type>] [--timeout <secs>] [--all]`            | mDNS scan for cameras on the LAN                                                                       |
| `probe <host>`                                                      | Connect, print the cert fingerprint and product info — run this first                                  |
| `get <host> <path>`                                                 | Any `GET`, e.g. `/video/iso` — the whole API is reachable this way                                     |
| `put <host> <path> <json>`                                          | Any `PUT`, e.g. `/video/iso '{"iso":800}'`                                                             |
| `record <host> start\|stop`                                         | Start/stop recording                                                                                   |
| `stream <host> status\|start\|stop\|available\|platforms\|platform` | Livestream: current state, go live/stop, availability + reasons, platform list, active platform config |
| `stream <host> dump`                                                | Every livestream setting the camera has, in one pass                                                   |
| `stream <host> target <url> [--quality <name>]`                     | Point the stream at your own RTMP/SRT receiver instead of a platform                                   |
| `watch <host> <property>...`                                        | Live `propertyValueChanged` events over the notification websocket, with reconnect                     |

| Option                   | Applies to       | Meaning                                                          |
| ------------------------ | ---------------- | ---------------------------------------------------------------- |
| `--fingerprint <sha256>` | any host command | Pin the connection to this cert (from `probe`)                   |
| `--insecure`             | any host command | Trust-on-first-use, no pinning — for `probe` only in practice    |
| `--user <u> --pass <p>`  | any host command | HTTP basic auth                                                  |
| `--service <type>`       | `discover`       | mDNS service type (default `_http._tcp.local.`)                  |
| `--timeout <secs>`       | `discover`       | How long to listen (default 5)                                   |
| `--all`                  | `discover`       | Unfiltered — every service instance, not just identified cameras |
| `--quality <name>`       | `stream target`  | Quality profile name; defaults to the platform's default         |

`<host>` is `cam.local`, an IP, or an explicit `http://cam.local`/`https://cam.local` to force
a scheme. Plain-HTTP hosts don't need `--fingerprint` — there's no certificate to pin.

### Typical session

```bash
bmcam probe http://Cinema-Camera-6K.local              # confirm reachable, get fingerprint
bmcam get http://Cinema-Camera-6K.local /system/product
bmcam record http://Cinema-Camera-6K.local start
bmcam record http://Cinema-Camera-6K.local stop
bmcam stream http://Cinema-Camera-6K.local dump          # every livestream setting at once
bmcam watch http://Cinema-Camera-6K.local /transports/0/record /video/iso
```

For an HTTPS camera, `probe` first without `--fingerprint` (it trust-on-first-uses and prints
the fingerprint), then pass `--fingerprint <that-value>` on every call after.

## Findings

### Discovery: service type and the macOS gate

Verified against a Cinema Camera 6K (firmware 10.2.2): cameras advertise on the **generic
`_http._tcp`**, not a vendor-specific service type, on **port 80**. They're told apart from
every other web server on the LAN by their TXT records — a camera carries `product name`,
`unique id`, `device name`, `release version`; a plain web server advertises no TXT at all.
Those TXT records mean discovery can label a multi-camera list without a single REST call.

**macOS caveat:** a process doing raw mDNS multicast gets nothing unless it holds Local
Network permission — `bmcam discover` from a terminal usually comes back empty even with
cameras present, while the system's own `dns-sd -B _http._tcp local.` lists them (it goes via
mDNSResponder instead). Embedding in the Tauri app needs `NSLocalNetworkUsageDescription` and
the multicast entitlement, or browsing silently finds nothing on macOS.

Adding by name or IP needs no multicast — `.local` resolves through the OS resolver — which is
why manual add is a first-class path here, not a fallback for when discovery fails.

### Certificate trust

Cameras present self-signed certificates by default, so there's nothing to validate against a
CA. `Trust::OnFirstUse` accepts whatever is presented and records its SHA-256;
`presented_fingerprint()` gives you what to show the operator. Once accepted, store it and
connect with `Trust::Pinned` — every later connection must present that exact certificate. A
camera that re-issues its certificate (firmware update, factory reset) comes back as
`Error::CertUntrusted` carrying the new fingerprint, to be re-accepted deliberately — the same
shape as an SSH host-key change. Nothing is ever added to a global trust store.

### Endpoint corrections (found by reading the manual, not guessed)

- Recording is `POST /transports/0/record` and `POST /transports/0/stop`.
  `PUT /transports/0/record` exists but the manual marks it **deprecated**.
- Livestream is `/livestreams/0/…` — plural and indexed — and `start`/`stop` are `PUT`.
- **This camera model has no Camera Control API group**, so there is no `/camera/tally`,
  colour bars, or battery endpoint. Those exist on Studio Camera models; here they'd 404/501.
- The manual's own account of the notification websocket's subscribable property list
  covers transport, video, lens, audio, monitoring, colour correction, presets and
  media — but not livestream or tally. **That's wrong for at least the Cinema Camera
  6K**: `GET /event/list` against real hardware includes `/livestreams/0`,
  `/livestreams/0/activePlatform` and `/livestreams/0/available` (tally is still
  absent — no Camera Control API group on this model, see above). `GET /event/list`
  is the authoritative list per device either way, since it varies by model; don't
  trust the manual's version of it.
- `properties` in a `subscribe` message is an array — the whole desired set goes in one
  message, so reconnect resubscribe is one message too, not N.
- The websocket URL is the one thing the manual never states; `/control/api/v1/event/websocket`
  — confirmed against hardware — is what `Camera::websocket_url()` uses.

### Live preview over the network

The camera has no preview, snapshot, or MJPEG endpoint. The only live video path off it is its
own streaming engine, which can point at a receiver on your LAN instead of a platform — the
manual notes local devices "will be available here when connected to the same local network".
So a preview is: camera → local RTMP/SRT server → your player.

```bash
# 1. run a receiver, e.g. mediamtx (single binary, RTMP/SRT in, WebRTC/HLS out)
# 2. point the camera at it and go live
bmcam stream http://Cinema-Camera-6K.local target rtmp://192.168.0.12/live/cam
bmcam stream http://Cinema-Camera-6K.local start
bmcam stream http://Cinema-Camera-6K.local status
```

`target` looks for a platform whose `customizableUrlEnabled` is true, selects it with
`server: "Custom"`, and sets your URL. If no platform allows a custom URL, the fallback is
uploading a Blackmagic streaming XML file (`PUT /livestreams/customPlatforms/{file}`, exported
from ATEM Setup) — the manual documents producing that file, not its schema.

Three limits worth knowing before building on this:

- **One stream, singular** (`/livestreams/0`). The camera cannot feed a local preview _and_
  stream to YouTube at once. Camera → local server → OBS → YouTube works; camera → both does not.
- **It's the program feed, not a low-latency preview.** ~1–3 s on RTMP/HLS, less on SRT/WebRTC.
  Fine for "where is the camera pointed", not for pulling focus.
- **Needs real bandwidth**, and this model has no built-in Ethernet — the USB-C adapter matters.

### Still unverified against real hardware

Blocked on Web Media Manager being enabled on the test camera — everything below is correct
per the manual but not yet round-tripped against a live device:

- Every REST call's actual response shape
- The notification websocket connect URL and message flow
- Whether any platform on this specific unit reports `customizableUrlEnabled: true`
- Whether an RTMP URL is accepted with no stream key

`bmcam stream <host> dump > livestream.txt` once it's reachable answers the streaming
questions in one shot.
