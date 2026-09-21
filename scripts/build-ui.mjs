#!/usr/bin/env node
/**
 * Builds the rendering UI(s) selected for this build and stages them into the
 * directory Tauri loads as `frontendDist`.
 *
 * Selection comes from `METOCAST_UI` (comma-separated registry ids); with no
 * value the registry's `default` is built, which is what keeps an ordinary
 * `pnpm tauri build` behaving exactly as it always did.
 *
 *   pnpm build:ui                    # the default UI
 *   METOCAST_UI=alt pnpm build:ui    # one other registered UI
 *   METOCAST_UI=default,alt pnpm ... # both, with a chooser in settings
 */

import { execSync } from 'node:child_process';
import { cpSync, existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const registry = JSON.parse(readFileSync(join(root, 'ui', 'registry.json'), 'utf8'));
const OUT = join(root, 'build');

const requested = (process.env.METOCAST_UI ?? registry.default)
  .split(',')
  .map((id) => id.trim())
  .filter(Boolean);

const selected = requested.map((id) => {
  const ui = registry.uis.find((candidate) => candidate.id === id);
  if (!ui) {
    const known = registry.uis.map((candidate) => candidate.id).join(', ');
    throw new Error(`Unknown UI "${id}". Registered: ${known}`);
  }
  return ui;
});

console.log(`[build-ui] building: ${selected.map((ui) => ui.id).join(', ')}`);

for (const ui of selected) {
  console.log(`[build-ui] ${ui.id}: ${ui.buildCommand}`);
  execSync(ui.buildCommand, { cwd: root, stdio: 'inherit' });
}

// A single UI is served from the root, byte-for-byte the layout Tauri has always
// loaded. Only a multi-UI build needs the per-UI subdirectories and a chooser.
if (selected.length === 1) {
  const [ui] = selected;
  if (resolve(root, ui.buildDir) !== OUT) {
    rmSync(OUT, { recursive: true, force: true });
    cpSync(resolve(root, ui.buildDir), OUT, { recursive: true });
  }
  writeFileSync(
    join(OUT, 'bundled-uis.json'),
    JSON.stringify({ active: ui.id, uis: [describe(ui, `/${ui.entry}`)] }, null, 2),
  );
  console.log(`[build-ui] staged ${ui.id} at build/`);
  process.exit(0);
}

// Collect outputs outside `build/` first: a UI whose buildDir *is* `build/`
// cannot be copied into a subdirectory of itself.
const STAGE = join(root, '.ui-staging');
rmSync(STAGE, { recursive: true, force: true });

const staged = [];
for (const ui of selected) {
  cpSync(resolve(root, ui.buildDir), join(STAGE, ui.id), { recursive: true });
  bootAtBundleRoot(join(STAGE, ui.id, ui.entry));
  staged.push(describe(ui, `/ui/${ui.id}/${ui.entry}`));
}

rmSync(OUT, { recursive: true, force: true });
mkdirSync(OUT, { recursive: true });
cpSync(STAGE, join(OUT, 'ui'), { recursive: true });
rmSync(STAGE, { recursive: true, force: true });

// SvelteKit writes asset URLs absolute, so every UI's appDir must also exist at
// the bundle root — distinct appDirs are what let them share it.
for (const ui of selected) {
  cpSync(join(OUT, 'ui', ui.id, ui.appDir), join(OUT, ui.appDir), { recursive: true });
}

// The copied `/presenter` URL must work whichever UI is active.
const withPresenter = selected.find((ui) => existsSync(join(OUT, 'ui', ui.id, 'presenter.html')));
if (withPresenter) {
  const presenterRoot = join(OUT, 'ui', withPresenter.id);
  cpSync(join(presenterRoot, 'presenter.html'), join(OUT, 'presenter.html'));
  const presenterFonts = join(presenterRoot, 'presenter-fonts');
  if (existsSync(presenterFonts)) {
    cpSync(presenterFonts, join(OUT, 'presenter-fonts'), { recursive: true });
  }
}

writeFileSync(
  join(OUT, 'bundled-uis.json'),
  JSON.stringify({ active: selected[0].id, uis: staged }, null, 2),
);
writeFileSync(join(OUT, 'index.html'), chooser(selected));
console.log(`[build-ui] staged ${staged.length} UIs under build/ui/`);

// Each UI is built for the bundle root (`base: ""`) and its assets are copied
// there, but its shell is served from `/ui/<id>/`. Left alone, the router reads
// that file path as a route and renders the UI's own 404.
function bootAtBundleRoot(entry) {
  const html = readFileSync(entry, 'utf8');
  writeFileSync(
    entry,
    html.replace(/<head[^>]*>/, `$&\n    <script>history.replaceState(null, '', '/');</script>`),
  );
}

function describe(ui, path) {
  return {
    id: ui.id,
    displayName: ui.displayName,
    description: ui.description ?? '',
    path,
  };
}

/**
 * Entry page for a multi-UI bundle: sends the window to the UI the user picked
 * in settings, falling back to the first one built. Runs once at start-up — the
 * choice applies on the next launch, not live.
 */
function chooser(uis) {
  const fallback = uis[0].id;
  const entries = Object.fromEntries(uis.map((ui) => [ui.id, ui.entry]));
  return `<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <title>Metocast</title>
    <script>
      (function () {
        var fallback = ${JSON.stringify(fallback)};
        var entries = ${JSON.stringify(entries)};
        var bundled = Object.keys(entries);
        var chosen = null;
        try {
          chosen = window.localStorage.getItem('metocast.activeUi');
        } catch (e) {
          /* storage unavailable — use the fallback */
        }
        var id = bundled.indexOf(chosen) === -1 ? fallback : chosen;
        window.location.replace('/ui/' + id + '/' + entries[id]);
      })();
    </script>
  </head>
  <body></body>
</html>
`;
}
