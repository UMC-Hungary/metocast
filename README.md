# Metocast

Church livestream control desktop application built with Tauri 2 + SvelteKit 5 + TypeScript.

## Rendering UIs

The front-end is swappable. Each rendering UI is a self-contained static app under `ui/` that
talks to the core through one package (`@metocast/core-client`) and never touches Tauri, `fetch`
or `WebSocket` directly, so the same UI runs in the desktop app, a browser, or a remote
client-mode window. This is a pnpm workspace:

```
ui/classic/            the original control surface (frozen; full coverage)
ui/sanctum/            the new Sanctum design UI
packages/core-client/  the only boundary to the core (framework-agnostic: HTTP, WS transport, Zod schemas, host, locales)
packages/design-system/ Sanctum's tokens + components (@metocast/design-system)
```

```bash
pnpm build                              # the registry's default UI (classic)
METOCAST_UI=sanctum pnpm build          # a different registered UI
METOCAST_UI=classic,sanctum pnpm build  # both, with a chooser + in-app selector
```

Registered UIs live in [`ui/registry.json`](ui/registry.json); [`ui/README.md`](ui/README.md) is
the contract for writing one and what belongs in the shared package.

## Rust workspace

The backend is a Rust 1.98.1 workspace with one-way dependencies:

```text
metocast-core          shared wire records and pure rules
├── metocast-client    typed HTTP/WebSocket client for native apps
│   └── metocast-apple UniFFI Swift bridge and XCFramework
└── metocast-server    Axum, PostgreSQL, workers, and connectors
    └── metocast       thin Tauri host in src-tauri
```

`metocast-core` is the source of truth for Rust wire types. The server owns persistence and
concrete integrations; it has no Tauri dependency. Both the desktop host and the headless
`metocast-server` binary start the same server library. The Apple client links only core/client
code and never embeds PostgreSQL or hardware connectors.

```bash
cargo build -p metocast-server --bin metocast-server
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
./scripts/build-apple.sh # creates apple/MetocastFFI.xcframework and runs swift test
```

When a public HTTP or WebSocket contract changes, update the core Rust record, server handler,
OpenAPI/Bruno examples, `@metocast/core-client` Zod schema, and affected native bridge record in
the same change.

## API access and secrets

The core exposes one HTTP/WebSocket API, used by the desktop app, remote client-mode UIs,
Companion and the presenter receiver alike. Access has three tiers:

| Tier                                      | Who                                                | What they get                                                                                                                                                                                                     |
| ----------------------------------------- | -------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **No credentials**                        | Anyone who can reach the port                      | `GET /health`, `/openapi.json`, `/docs`, the OBS caption overlay (`/caption`), the OAuth callback, and the UI bundle. A WebSocket may connect without a token but is limited to the read-only presenter commands. |
| **Auth token**                            | The desktop app, client-mode UIs, Companion        | Everything under `/api/*` and full WebSocket access, via `Authorization: Bearer <token>` (or `?token=` for `/ws`). The token is shown on the Connect page.                                                        |
| **Auth token + admin token, on loopback** | The desktop app hosting the server, in server mode | Reading stored upstream credentials back.                                                                                                                                                                         |
| **Nobody**                                | —                                                  | Everything else about credentials. See below.                                                                                                                                                                     |

**Upstream credentials never leave the server.** The szentiras.eu API key, YouTube client
secret, Facebook app secret, OBS and Blackmagic camera passwords, and the Discord webhook URL
are stored server-side and used only for the core's own outbound requests.
`GET /api/connectors/{name}/config` always returns them blank, with a `<field>Set` boolean
saying whether one is stored — holding the auth token is not enough to read them. When saving,
send a new value to replace a secret, leave it blank to keep the stored one, or send
`"<field>Set": false` to clear it.

The one exception is the machine actually running the server: its own desktop app can re-read a
credential it stored, via `GET /api/connectors/{name}/config/secrets`. That needs a second
admin token — regenerated every run, kept in memory, handed to the host window over Tauri IPC and
never over the network — _and_ the request must arrive on loopback. A client-mode window is
talking to someone else's core, so it never gets one, and the **Show** action stays hidden.

Full contract, including what is deliberately public and what is still open (per-device keys,
encryption at rest, TLS): [plans/PLAN-api-access-contracts.md](plans/PLAN-api-access-contracts.md).

## Presenter Receiver

A standalone binary that connects to the Metocast server over WebSocket and renders slides directly on a display — no browser required. Designed for Raspberry Pi / Linux framebuffer setups or macOS secondary screens.

Presentations have two persisted designs: **Classic** and **Editorial**. Editorial uses the shared warm-black, ivory and antique-gold treatment for both imported song decks and generated Bible slides; choose it from the Presentations settings in either Tauri UI. The presenter keeps both the source SVG and extracted text for imported decks, so switching the design updates the live output immediately: Classic shows the source artwork and Editorial shows the styled text.

In Sanctum, use **Slides → + → Create song PPT**, enter a title, and paste lyrics with two empty lines between slides. Metocast keeps every entered line intact, adds a title slide and a final blank slide, and writes the generated `.pptx` into the song output folder shown in **Slides → Settings**. Bible decks continue to use their separate folder under **Settings → Event settings**.

See [presenter-receiver/PRESENTER_RECEIVER.md](presenter-receiver/PRESENTER_RECEIVER.md) for full documentation: installation, auto-start on boot, supported platforms, update instructions, and WebSocket protocol reference.

### Quick start

```bash
# Install
curl -fsSL https://raw.githubusercontent.com/UMC-Hungary/sermon-helper-applications/main/presenter-receiver/install.sh | bash -s -- ws://YOUR_SERVER_IP:3737/ws

# Or with authentication token
presenter-receiver ws://192.168.1.10:3737/ws --token YOUR_TOKEN

# Local development against the Tauri backend
cargo run --manifest-path presenter-receiver/Cargo.toml --bin presenter-receiver -- \
  ws://127.0.0.1:3737/ws --token YOUR_TOKEN
```

## Companion Module

The Companion module communicates with Metocast only through the app WebSocket. Its Textus and Lekcio presets call `presenter.load_bible_reference` without an `event_id`, so the backend-selected current/next event is used.
The active Classic/Editorial presenter design is selected in Metocast and is applied automatically when Companion opens a text-mode song or Bible presentation.

RØDECaster audio capture is core-owned and uses the authenticated WebSocket commands
`rodecaster.audio.discover`, `rodecaster.audio.record.start` (with `event_id`),
`rodecaster.audio.record.stop`, and `rodecaster.audio.record.state`. It writes lossless FLAC files
through `.partial` paths, atomically finalizes them, and records source/mapping provenance on the
event media row.

The RØDECaster channel profile keeps the mixer's `mute` separate from the Wireless PRO
transmitter's `wirelessMute`. Live `rodecaster.mute` events identify transmitter-button changes
with `remote: true`; the UI flags those as **Remote mic muted** without changing the channel toggle.

Middle Control connects over its line-oriented TCP External API and remains the sole transport to
APC-R / APC-R Mini units. Its host and port are configured under Connectors (current macOS default
`11584`, observed Middle Control 3.2.0 macOS `11581`, Windows `11580`). The shared WebSocket accepts
typed `middlecontrol.camera.select`, `middlecontrol.record.start`/`stop`,
`middlecontrol.record.start_all`/`stop_all`, and `middlecontrol.preset.recall` commands; live camera,
recording, APC-R presence, and preset-move state arrive in `connector.state` messages. Use
`localhost` when Middle Control runs on the Metocast server computer. The authenticated
`POST /api/connectors/middlecontrol/discover` scan checks localhost and the server's local `/24` on
the known ports, and returns only endpoints that send a valid Middle Control feedback frame.

The Blackmagic ATEM connector speaks the switcher's UDP control protocol (default port `9910`) and
was verified against an ATEM Mini Pro (protocol 2.31). Enable it under Connectors with the
switcher's IP address, or find it with the authenticated `POST /api/connectors/atem/discover`,
which browses mDNS for `_switcher_ctrl._udp` and finds switchers both on the LAN and plugged into
the server by USB (the switcher's USB-C port brings up a USB Ethernet link that carries the same
protocol; those results are flagged `usb: true`). Program, preview, input labels, streaming and recording state arrive
unprompted in `connector.state` messages (`connector: "atem"`, `state: {…}`); models without a
streaming or recording engine report those as `null`. The shared WebSocket accepts
`atem.program.set` / `atem.preview.set` (with `input`), `atem.cut`, `atem.auto`,
`atem.record.start`/`stop`, `atem.stream.start`/`stop`, and `atem.stream.push_youtube`.
`POST /api/connectors/atem/stream/youtube` writes the channel's ingestion address and stream key into
the switcher's streaming service without going live.

## Development

```bash
pnpm dev              # Vite dev server only (port 1420)
pnpm tauri dev        # Full Tauri desktop app in dev mode
pnpm tauri build      # Production build
pnpm build:server     # Headless server binary (target/debug/metocast-server)
pnpm check            # TypeScript + Svelte type checking
```
