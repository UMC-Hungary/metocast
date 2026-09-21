# AGENTS.md

This file provides guidance to Codex (Codex.ai/code) when working with code in this repository.

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
- Uses Tauri plugins: store, dialog, opener, liquid-glass. The Tauri store holds host-side settings only (mode, server URL/token, log paths); it is not the core's config store.

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
- Always name plan files with the format: PLAN-{feature-name}.md under the `plans` folder.
- Always use Zod for runtime validation of external, persisted, IPC, API, and WebSocket data.
- Always keep strict TypeScript types. Do not use `any`, broad casts, or relaxed compiler settings to bypass type errors.
- Respect server/client mode boundaries: server mode owns Postgres, Axum, connectors, schedulers, and uploads; client mode must call the configured server through typed HTTP/WebSocket clients rather than starting local backend services.
- Companion communication must go through the app WebSocket protocol. Do not add REST-only, filesystem, or ad hoc side channels for Companion control or status.
- Upstream credentials (API keys, OAuth secrets, passwords, webhook URLs) must never be readable over the API, except through `GET /api/connectors/{name}/config/secrets`, which requires the admin token _and_ a loopback caller and exists so the host desktop app can re-read what it stored. Add new secret field names to `SECRET_FIELDS` in `src-tauri/src/server/routes.rs` so reads are blanked and blank writes keep the stored value, and never log them. See [plans/PLAN-api-access-contracts.md](plans/PLAN-api-access-contracts.md) for the full access contract.
- New HTTP routes must be added to one of the two lists in `e2e/rest/auth.test.ts` (public or authenticated). Routes merged into the `/api` router _after_ its `route_layer(auth_middleware)` call silently bypass authentication — merge before the layer.
- Any REST or WebSocket contract change must update all matching contract surfaces in the same change: OpenAPI/Swagger generation in `src-tauri/src/server/openapi.rs`, Bruno requests under `bruno/`, project README/docs, frontend Zod schemas and API/WS clients, the presenter web view, the `presenter-receiver` terminal view, and the Companion plugin. Keep these surfaces in sync; do not leave protocol drift for a later task.
- When fixing release workflow problems, do not mint a new version tag for each CI attempt. Finish the workflow fix first, then republish the latest synchronized release tag set so the app, companion, and presenter receiver tags stay on the same version.
