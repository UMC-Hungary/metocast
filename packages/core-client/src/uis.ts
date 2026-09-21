/**
 * Which rendering UIs this build bundled, and which one this window opens.
 *
 * `bundled-uis.json` is written by `scripts/build-ui.mjs` beside the bundles;
 * a dev server has no such file, which is simply "nothing to choose".
 */

const ACTIVE_UI_KEY = 'metocast.activeUi';

export interface BundledUi {
  id: string;
  displayName: string;
  description: string;
  path: string;
}

export interface BundledUis {
  /** The UI opened when the user has not chosen one. */
  active: string;
  uis: BundledUi[];
}

export async function loadBundledUis(): Promise<BundledUis | null> {
  try {
    const res = await fetch('/bundled-uis.json');
    if (!res.ok) return null;
    return (await res.json()) as BundledUis;
  } catch {
    return null;
  }
}

export function getActiveUi(fallback: string): string {
  try {
    return localStorage.getItem(ACTIVE_UI_KEY) ?? fallback;
  } catch {
    return fallback;
  }
}

/** Applies on the next start — the chooser page reads this at launch. */
export function setActiveUi(id: string): void {
  try {
    localStorage.setItem(ACTIVE_UI_KEY, id);
  } catch {
    // Storage unavailable; the bundle keeps opening its default UI.
  }
}
