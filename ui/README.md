# Writing a rendering UI

A rendering UI is a static front-end that talks to the Metocast core. The core does not know
or care which one is running: the desktop app, a browser tab, and a remote client-mode window
all speak the same HTTP/WebSocket API. Each UI is a **self-contained application** under `ui/`
(its own `package.json`, build config and source), and shared code lives in `packages/`. This
document is the contract a UI has to satisfy.

Registered UIs today: `ui/classic/` (the original control surface, frozen) and `ui/sanctum/`
(the new design). Neither is privileged by its location.

## The rules

1. **Reach the core only through `@metocast/core-client`.** Never import `@tauri-apps/*`, never
   call `fetch` directly, never construct a `WebSocket`. ESLint enforces this for everything
   under `ui/**`; `packages/core-client` is the one place those are allowed, so the rule applies
   to any new UI automatically.
2. **Do not import from another UI.** Anything two UIs share goes in a workspace package they
   both depend on by name, never a relative path into a sibling UI.
3. **Assume no desktop shell.** Anything Tauri-only is a _host capability_: which core the
   window talks to, log files, updates, native dialogs. Check `hostCapabilities` before
   offering it and hide or degrade the control when it is absent. A UI that throws outside the
   desktop app is broken, not "desktop-only".
4. **Validate at the boundary.** Every HTTP response and WebSocket message is parsed with a Zod
   schema, and the TypeScript types are `z.infer`red from those schemas. Do not hand-write an
   interface for something that crossed the wire.
5. **Build to static files.** No server-side rendering at runtime — the output is plain files
   that Tauri embeds and the core serves. Both current UIs use `@sveltejs/adapter-static`.
6. **Change no Rust.** If a UI needs something the API does not expose, that is a core change
   with its own contract-surface updates (`metocast-core`, server handler, OpenAPI, Bruno, Zod
   schemas, and native bridge when applicable), not a shortcut through IPC.

## The shared package

`packages/core-client` (`@metocast/core-client`) is the single boundary to the core. It is
**framework-agnostic** — a React UI could consume it as easily as a Svelte one — so the test for
what belongs there is simple: _if it imports a UI framework, it does not belong._

| Behind it                                    | What                                                                                                                                   |
| -------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------- |
| HTTP (`src/api`)                             | Events, recordings, uploads, queues, connector config/status, Bible lookups, presenter                                                 |
| WebSocket (`src/ws`)                         | `connectWs` (a **transport**: connect/reconnect/validate, emitting typed messages via handlers), `sendWsCommand`, `connectPresenterWs` |
| Host (`src/host`)                            | `hostCapabilities` plus the optional desktop-only calls, each feature-detected                                                         |
| Schemas / types (`src/schemas`, `src/types`) | One Zod definition per message; a backend change is a compile error in whichever UI drifts                                             |
| Config (`src/config`)                        | `configureCoreClient` — the UI supplies the core's location/token once at startup                                                      |
| Locales (`locales/`)                         | Shared translation catalogues; a UI adds its own keys but shares these                                                                 |

What stays in each UI: its stores, components, routes, styling, notifications — and the
**bindings** that write incoming WebSocket messages into that UI's own state (the transport is
shared; what it does with a message is not). `packages/design-system` (`@metocast/design-system`)
is Sanctum's component library and is not part of the core boundary.

## Registering a UI

Add an entry to [`registry.json`](registry.json):

```json
{
  "id": "my-ui",
  "displayName": "My UI",
  "description": "One line, honest about what it does and does not cover.",
  "buildCommand": "pnpm --filter @metocast/my-ui build",
  "buildDir": "ui/my-ui/build",
  "appDir": "_my-ui",
  "entry": "index.html"
}
```

| Field          | Meaning                                                                         |
| -------------- | ------------------------------------------------------------------------------- |
| `id`           | Stable identifier; used in `METOCAST_UI` and in the bundle path                 |
| `displayName`  | Shown in the settings selector                                                  |
| `description`  | One-liner under the name — describe coverage honestly, since UIs need not match |
| `buildCommand` | Run from the repo root; must produce static files                               |
| `buildDir`     | Where that command writes them, relative to the repo root                       |
| `appDir`       | The UI's SvelteKit `kit.appDir`; **must be unique** across the registry         |
| `entry`        | The HTML file to open, relative to `buildDir`                                   |

`appDir` is what makes a multi-UI bundle possible. SvelteKit writes asset URLs absolute
(`/_my-ui/immutable/...`), so every bundled UI's asset directory has to sit at the bundle root —
leave two UIs on the default `_app` and one overwrites the other.

## Building

`scripts/build-ui.mjs` runs as `pnpm build` (which `tauri build` invokes) and reads
`METOCAST_UI`:

```bash
pnpm build                              # the registry's default UI (classic)
METOCAST_UI=sanctum pnpm build          # one other registered UI
METOCAST_UI=sanctum,classic pnpm build  # both, with a chooser in settings
```

With a single UI the output is staged at `build/` exactly as before — an ordinary build is
unchanged. With several, each lands in `build/ui/<id>/` with its `appDir` copied to the bundle
root and its shell rewriting the URL back to `/` on load — a UI is built for the root it serves
its assets from, and a router left at `/ui/<id>/index.html` reads that as a route and 404s.
`build/bundled-uis.json` lists them,
and `build/index.html` becomes a small chooser page that sends the window to the UI selected in
each UI's own settings. Every bundled UI must offer that selector (over the shared
`metocast.activeUi` key), so no UI can be entered without a way out. Selecting a different UI
persists the choice and **loads it immediately** — the chosen UI is its own bundle, so switching
navigates to it rather than waiting for the next launch. The **first id listed** is the one a
fresh install opens, before anyone has chosen.

Release builds set `METOCAST_UI: sanctum,classic` in `.github/workflows/build.yml`, so every
installer ships both UIs and opens Sanctum by default.
