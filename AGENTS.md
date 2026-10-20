# AGENTS.md

This file provides guidance to AI coding agents (Claude Code, Codex) when working with code in this repository. `CLAUDE.md` imports it, so the rules below reach Claude sessions too.

## Project Overview

Church livestream control desktop application built with **Tauri 2 + SvelteKit 5 + TypeScript**.

## Commands

```bash
pnpm dev              # dev server for the default UI, classic (port 1420)
METOCAST_UI=sanctum pnpm dev          # dev server for sanctum instead (also 1420)
pnpm tauri dev                        # desktop app in dev — launches classic
METOCAST_UI=sanctum pnpm tauri dev    # desktop app in dev — launches sanctum
pnpm tauri build      # Production build (bundles the registry's default UI: classic)
pnpm check            # Type-check every workspace UI
pnpm lint             # Lint the whole workspace

# Which UI(s) a build bundles (scripts/build-ui.mjs reads METOCAST_UI):
pnpm build                              # default UI (classic)
METOCAST_UI=sanctum pnpm build          # one other registered UI
METOCAST_UI=classic,sanctum pnpm build  # both, with a chooser + in-app selector
```

## Architecture

**Tech Stack:** SvelteKit 2.9 + Svelte 5 + Tauri 2 + TypeScript + Zod + Rust/Axum + PostgreSQL

**Workspace layout (pnpm workspace):**

The frontend is no longer at the repo root. Each rendering UI is a self-contained app under `ui/`, and shared code lives in `packages/`:

- `ui/classic/` - the original SvelteKit control surface (frozen; full feature coverage). Its own `package.json`/`svelte.config.js`/`vite.config.js`/`tsconfig.json`, source under `ui/classic/src/`.
- `ui/sanctum/` - the new Sanctum design UI (Svelte 5). Screens are built by the separate `sanctum-ui` change.
- `packages/core-client/` - **the only way a UI reaches the core.** Framework-agnostic: HTTP operations, the WebSocket _transport_ (validate + emit typed events — each UI binds those to its own stores), Zod schemas, the optional desktop host adapter, and the shared locale catalogues. Imported as `@metocast/core-client`.
- `packages/design-system/` - Sanctum's tokens + accessible components, with Storybook as its catalog. Imported as `@metocast/design-system` (`./tokens.css`, `./fonts.css`). Consumed by `ui/sanctum` only.
- `ui/registry.json` + `scripts/build-ui.mjs` - registered UIs and the build/staging/chooser machinery.

> Path note: architecture paths written below as `src/lib/...` now live under `ui/classic/src/lib/...` (UI-specific: routes, components, stores) or `packages/core-client/src/...` (shared: `api`, `ws`, `host`, `schemas`, `types`, `utils`).

**Backend (`/src-tauri`):**

- Rust is the core application backend. It owns the Axum HTTP/WebSocket server, embedded PostgreSQL lifecycle, migrations, models, schedulers, upload workers, and connector orchestration.
- `server/` - Axum routes, auth, OpenAPI, WebSocket protocol, presenter/PPT/caption endpoints
- `database/` - embedded PostgreSQL startup plus SQLx migrations
- `connectors/` - OBS, Blackmagic cameras (REST + livestream control, via the `blackmagic-camera` crate), YouTube, Facebook, Broadlink, Keynote, ATEM/vMix/Discord integrations, plus Szentírás.eu (config-only: it just holds the Bible API key)
- `runtime.rs` - shared display-free bootstrap (Postgres → migrations → pool → connectors → Axum) used by both the Tauri app and the headless `metocast-server` binary (`bin/metocast-server.rs`)
- Connector configuration lives in the `app_settings` table and is read/written over HTTP (`GET`/`PUT /api/connectors/{name}/config`), not through Tauri IPC, so a headless core and remote UIs see the same values
- `bible.rs` - Bible passage lookups against the upstream Hungarian APIs, exposed as `GET /api/bible/verses` and `/api/bible/suggest`. UIs never call the upstream directly, so no UI needs a CORS workaround. Classic translations (RUF, KG, KNB, SZIT, BD, STL) come from szentiras.eu, which requires an `X-API-Key` held by the `szentiras` connector config; `*_v2` translations and reference autocomplete need no key
- `uploader/` and `scheduler/` - resumable upload orchestration and cron-triggered automation
- `server/mcp.rs` - the MCP server at `POST /mcp`: tools are built from `WsCommand` (schemars) and a REST catalogue, and run through the real `/api` router and `handle_ws_command`; see [docs/mcp/README.md](docs/mcp/README.md) and [plans/PLAN-mcp-server.md](plans/PLAN-mcp-server.md)
- `crates/metocast-mcp-bridge` - the stdio bridge for MCP clients that can only start a program (Claude Desktop), run as `metocast-server mcp-stdio` by a first-line dispatch in `bin/metocast-server.rs`. It is a dumb pipe to `POST /mcp` that keeps the session, re-initializes when the server forgets it, and turns failures into readable JSON-RPC errors; see [docs/mcp/BRIDGE.md](docs/mcp/BRIDGE.md) and [plans/PLAN-mcp-stdio-bridge.md](plans/PLAN-mcp-stdio-bridge.md)
- `automations/` (in `crates/metocast-server/src/`) - connector automations: rules that fire when a Middle Control preset finishes moving or a RØDECaster channel goes live or silent, move a camera and start or stop recorders. Pure decisions are in `matching.rs`, the recorder calls shared with the WebSocket handlers in `recording_control.rs`; see [plans/PLAN-connector-automations.md](plans/PLAN-connector-automations.md)
- Uses Tauri plugins: store, dialog, opener. The Tauri store holds host-side settings only (mode, server URL/token, log paths); it is not the core's config store.

**Key Integrations:**

- `obws` - Rust OBS WebSocket client for streaming/recording state and device scanning
- Axum WebSocket `/ws` - primary realtime control/event channel for the UI, Companion module, and presenter receiver
- Embedded PostgreSQL via `postgresql_embedded` + `sqlx`
- `svelte-sonner` - Toast notifications

**PostgreSQL Features:** The app uses PostgreSQL UUID primary keys generated with `pgcrypto`, `TIMESTAMPTZ`, foreign keys with cascading deletes, composite primary keys, partial indexes for pending upload work, JSON/JSONB-style payload storage through SQLx JSON support, PL/pgSQL trigger functions, and `pg_notify`/LISTEN notifications to broadcast database changes to WebSocket clients.

**WebSocket Protocol:** `src-tauri/src/server/websocket.rs` defines the command protocol for events, recordings, activities, cron jobs, uploads, connectors, Broadlink, OBS device listeners, presenter control, client registry, ping/pong latency, notifications, and presentation state broadcasts. Frontend message schemas live in `packages/core-client/src/schemas/ws-messages.ts` (shared by every UI).

**Server/Client Mode Architecture:** The app can run in `server` or `client` mode. Mode is persisted through Tauri commands in `src-tauri/src/commands/server.rs` and mirrored in `ui/classic/src/lib/stores/mode.ts`. In `server` mode, the desktop app starts the embedded PostgreSQL database and Axum server on the configured port, owns connector workers, exposes localhost and LAN URLs, serves HTTP APIs, and broadcasts realtime state over `/ws`. In `client` mode, the app does not start the backend stack; it stores a remote server URL plus auth token and the Svelte frontend talks to that server via typed HTTP API calls and the same `/ws` WebSocket protocol. `ui/classic/src/lib/components/layout/ConnectorInit.svelte` resolves the mode at startup, sets `serverUrl`, `authToken`, `localNetworkUrl`, and connector status sources, then opens the WebSocket (via `ui/classic/src/lib/ws-bindings.ts`, which binds the shared transport to classic's stores). Sanctum wires the same config through `configureCoreClient` in `ui/sanctum/src/lib/core.ts`. The Connect page only exposes token/network install details in server mode.

## Key Patterns

**State Management:** Svelte stores (writable/derived) in `/lib/stores/`, props for component communication

**Styling:** Plain CSS with app-level CSS variables in `src/app.css`; Tailwind is not part of the active dependency stack.

**Runtime Validation:** Zod schemas define API, WebSocket, and persisted data boundaries. Infer TypeScript types from Zod schemas with `z.infer` whenever a value crosses a runtime boundary.

**Type Safety:** TypeScript runs in strict mode with `noUncheckedIndexedAccess`, `noImplicitOverride`, and `exactOptionalPropertyTypes`. Keep types explicit at module boundaries and avoid weakening strictness.

**SystemStatus Type** (`src/lib/stores/types.ts`):

```typescript
type SystemStatus = {
  obs: boolean;
  rodeInterface: boolean;
  mainDisplay: boolean;
  secondaryDisplay: boolean;
  youtubeLoggedIn: boolean;
};
```

## Implementation Status

**Complete:** Sidebar, UI components, OBS WebSocket integration, toast system, error messages display, Service Events

**Stubs:** Bible editor

## Type Definitions

Two SystemStatus definitions exist:

- `src/lib/stores/types.ts` - Flat structure (correct)
- `src/lib/types.ts` - Nested structure (outdated, to be consolidated)

## Rules

- Always fix all `pnpm check` errors before finishing, even if they are unrelated to your changes.
- UI code (anything under `ui/**`) reaches the core only through `@metocast/core-client`. No `@tauri-apps/*` imports, no raw `fetch`, no `new WebSocket` — ESLint enforces this for every UI, and only `packages/core-client` is exempt. A UI must not import from another UI's directory; shared code goes in a workspace package. Desktop-only features are host capabilities: gate them on `hostCapabilities` and degrade when absent. See `ui/README.md` for the UI authoring contract.
- Always finish with zero warnings and zero errors from relevant Rust and frontend checks. Do not add `#[allow(dead_code)]`, `#![allow(dead_code)]`, or similar warning suppressions; remove or restructure unused code instead.
- **Never suppress anything, anywhere.** A warning, lint, type error, deprecation or build message is fixed at its cause or it stays visible; it is never hidden, silenced, downgraded or filtered out. That rules out, in any language, config file, script or workflow:
  - directives: `#[allow(...)]` and `#[expect(...)]`, `// eslint-disable` and `<!-- eslint-disable -->`, `@ts-ignore`, `@ts-expect-error` and `@ts-nocheck`, `svelte-ignore`, `swiftlint:disable`, `-Wno-*` and `#pragma ... ignored`;
  - weakened settings: a lint rule set to `off`, `warn` or an ignore list that covers source, `skipLibCheck` or any other relaxed compiler setting, a build setting that skips a diagnostic (for example `LM_SKIP_METADATA_EXTRACTION`);
  - hidden output or failures: `|| true`, `2>/dev/null`, `>/dev/null` on a command that can warn, `--silent` and `--quiet`, `continue-on-error`, and `if-no-files-found` set to `warn` or `ignore`;
  - casts, stubs or wrappers whose only job is to make a type error go away.

  Fix the cause: change the code, or the data, or use the API the rule asks for (`resolve()` for navigation, `SvelteSet` for a reactive set). If the cause is in a third-party package or the toolchain and cannot be fixed here, leave the diagnostic visible, say so in the pull request, and do not weaken the check to get past it. Run the whole check after the change and read all of it, never only the last line.

- Always build the native Apple app, for the Mac and for the iPhone Simulator, before finishing a change that can reach it, and read the whole output, not only the last line. That is any change under `apple-app/`, `apple/`, `crates/metocast-apple/`, `crates/metocast-server/` (its binary is the helper embedded in the Mac app), `scripts/embed-apple-server.sh` or `scripts/mcpb/`, and any REST, WebSocket or MCP contract change the app consumes. `** BUILD SUCCEEDED **` is not enough: the build must print **no `error:`, no `warning:`, no `WARNING` and no `note:`**, apart from the one listed below. Fix the cause; do not suppress a warning, lower a deployment target or add `-quiet`. The commands are the ones CI runs (`.github/workflows/rust-libraries.yml`), with the architecture pinned so Xcode does not warn about two matching destinations:

  ```bash
  # Mac
  xcodebuild -project apple-app/Metocast.xcodeproj -scheme Metocast -configuration Debug \
    -destination 'platform=macOS,arch=arm64' -derivedDataPath target/apple-app-derived build 2>&1 | tee /tmp/mac-build.log
  # iPhone Simulator
  xcodebuild -project apple-app/Metocast.xcodeproj -scheme Metocast \
    -destination 'generic/platform=iOS Simulator' -derivedDataPath target/apple-app-derived \
    CODE_SIGNING_ALLOWED=NO build 2>&1 | tee /tmp/ios-build.log
  # Must print nothing but the accepted messages below:
  grep -nE "error:|warning:|WARNING|note: " /tmp/mac-build.log /tmp/ios-build.log \
    | grep -vE "Target dependency graph|Building targets in dependency order|Emplaced|Using stub executor"
  ```

  Three messages are accepted. All are printed on every build and stay in the log: an accepted message is documented, never filtered out or hidden.
  1. `note: Run script build phase 'Embed Metocast Server' will be run during every build…`: that phase has to run every build because it copies the helper, so it cannot declare outputs.
  2. `warning: Metadata extraction skipped. No AppIntents.framework dependency found.`, printed once per build by Xcode's own `appintentsmetadataprocessor`. The app declares no App Intents, so the step has nothing to extract. The only ways to remove the message are an undocumented build setting (`LM_SKIP_METADATA_EXTRACTION`, which would hide it) or a dependency on `AppIntents.framework` that the app does not use, and both are worse than seeing it. When the app first adds an App Intent or App Shortcut the message goes away: delete this entry then, and it must not be re-added by a setting.
  3. `note: detected encoding of input file as Unicode (UTF-8)`, once for each `.strings` and `.stringsdict` file Xcode compiles from the app's String Catalogs (`Localizable.xcstrings`, `InfoPlist.xcstrings`), in each language. `xcstringstool` writes those files as XML property lists, and Xcode's `CopyStringsFile` step prints the note whenever it works out their encoding itself. Telling it the encoding (`STRINGS_FILE_INPUT_ENCODING = UTF-8`) fails the build with `could not decode input file using specified encoding`, because the step then reads the property list as a plain `.strings` file, so the note stays until Xcode stops printing it.

  Any other line, in either log, fails the change. When `crates/metocast-apple` changed, run `scripts/build-apple.sh` first so the Swift bindings and XCFramework are regenerated. Because that phase rewrites the helper on every build, an *incremental* build can leave the app's seal stale (`codesign --verify` says "a sealed resource is missing or invalid"); verify the signature on a build into a fresh `-derivedDataPath`, and expect it to be silent: `codesign --verify --deep --strict <Metocast.app>`. A new Swift warning, a new Xcode build setting that prints a message, or a new build phase that prints one is a regression to fix in the same change, and a message that cannot be fixed must be added to the accepted list in this rule with its reason.
- Links and `goto()` in a UI go through `resolve()` from `$app/paths` (the ESLint rule `svelte/no-navigation-without-resolve` is on): `href={resolve('/events/[id]', { id })}`. A link to an external URL carries `rel="external"`. No HTML string is ever built for `{@html}`: parse into data and render it with the template.
- Always name plan files with the format: PLAN-{feature-name}.md under the `plans` folder.
- Always use Zod for runtime validation of external, persisted, IPC, API, and WebSocket data.
- Always keep strict TypeScript types. Do not use `any`, broad casts, or relaxed compiler settings to bypass type errors.
- Respect server/client mode boundaries: server mode owns Postgres, Axum, connectors, schedulers, and uploads; client mode must call the configured server through typed HTTP/WebSocket clients rather than starting local backend services.
- Companion communication must go through the app WebSocket protocol. Do not add REST-only, filesystem, or ad hoc side channels for Companion control or status.
- Upstream credentials (API keys, OAuth secrets, passwords, webhook URLs) must never be readable over the API, except through `GET /api/connectors/{name}/config/secrets`, which requires the admin token _and_ a loopback caller and exists so the host desktop app can re-read what it stored. Add new secret field names to `SECRET_FIELDS` in `src-tauri/src/server/routes.rs` so reads are blanked and blank writes keep the stored value, and never log them. See [plans/PLAN-api-access-contracts.md](plans/PLAN-api-access-contracts.md) for the full access contract.
- New HTTP routes must be added to one of the two lists in `e2e/rest/auth.test.ts` (public or authenticated). Routes merged into the `/api` router _after_ its `route_layer(auth_middleware)` call silently bypass authentication — merge before the layer.
- Any REST or WebSocket contract change must update all matching contract surfaces in the same change: OpenAPI/Swagger generation in `src-tauri/src/server/openapi.rs`, Bruno requests under `bruno/`, project README/docs, frontend Zod schemas and API/WS clients, the presenter web view, the `presenter-receiver` terminal view, and the Companion plugin. Keep these surfaces in sync; do not leave protocol drift for a later task.
- When fixing release workflow problems, do not mint a new version tag for each CI attempt. Finish the workflow fix first, then republish the latest synchronized release tag set so the app, companion, and presenter receiver tags stay on the same version.
- Any REST route or `WsCommand` variant change keeps the MCP surface whole, in the same change: a new `/api` route needs a row in `REST` or `EXCLUDED` in `crates/metocast-server/src/server/mcp.rs` (a new WebSocket command becomes a tool by itself, and needs only a `///` summary and the right access hints); add any new push message to `server/ws_events.rs` with its fields; regenerate `docs/mcp/TOOLS.md`, `docs/api/WEBSOCKET.md` and `docs/api/ws-events.schema.json` with `UPDATE_GOLDEN=1 cargo test -p metocast-server --lib`. `cargo test -p metocast-server` must pass `mcp_covers_everything`, `openapi_covers_router`, `ws_events_catalogued` and the golden-file tests; `e2e/ws/event-payloads.test.ts` checks the live messages against the schema file. Extend `e2e/mcp/` when a tool needs behaviour beyond what the bridge gives it.
- A change to the MCP protocol or to session handling in `server/mcp.rs` must keep `metocast-server mcp-stdio` working: `cargo test -p metocast-mcp-bridge`, `cargo test -p metocast-server --test mcp_stdio` and `e2e/mcp/stdio-bridge.test.ts` pass. The bridge is built into the app's helper, so it always carries the app's version. `mcp-stdio` is dispatched first in `main`, before logging, the token check and the data directory: it must never start PostgreSQL, bind a port or create a directory (a failed PostgreSQL start kills whatever holds the port, the running app's database included).
- New operations that stop, delete or transmit something are destructive: make sure the generated hints say so (`destructive_tools_are_marked` lists the examples), and read the result in `docs/mcp/TOOLS.md`.
- Every change that adds or changes an operator-visible feature updates its page under `docs/` in the same change: what it does, how to use it, and its REST, WebSocket and MCP entry points. The MCP feature's page is `docs/mcp/README.md`. Screenshots go under `docs/images/<feature>/`, taken with `scripts/docs-screenshot.sh` from the Mac test copy launched with `-MetocastRedact YES` (it swaps the computer name, LAN addresses and token for placeholders; the script refuses to capture an app that was not launched that way) and checked with `swift scripts/check-screenshots.swift docs/images`; `git config core.hooksPath .githooks` turns on the pre-commit hook that runs it. `METOCAST_REDACT=1` makes the server itself replace the computer name, home folder, LAN addresses and tokens in everything it sends (REST, WebSocket, MCP), for screenshots of any UI. Never commit a screenshot that shows a computer name, user name, LAN IP or token.
- Never discard a `Result` in Rust. `let _ = <something fallible>;`, `.ok();` and `drop(<Result>)` are not allowed: they hide the failure from every log. Decide what a failure means at that site. If it can be acted on, act: return it, send the client an error, stop the loop or the stream. If nobody can act on it, say so with the helpers in `metocast_core::best_effort`: `.log_gone()` when the other side is gone (a closed channel or socket, an exited child; logged at `trace`) and `.log_failed()` when an operation failed and there is no recovery (a flush, a write, a delete; logged at `warn` with the error). `remove_file_if_present` and `remove_dir_all_if_present` (and `crate::cleanup::remove_file_if_present` in async server code) remove something that may already be gone and only log a failure that is not `NotFound`. A `tokio::sync::watch` sender uses `send_replace`, which cannot fail. Crates that do not depend on `metocast-core` (the standalone CLIs and libraries) handle it in place with `if let Err(error) = ... { tracing::warn!(%error, "...") }`, or `eprintln!` in a binary.
