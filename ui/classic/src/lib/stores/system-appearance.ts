import { enableWindowGlass, watchHostTheme } from '@metocast/core-client';
import { writable } from 'svelte/store';

export const systemTheme = writable<'light' | 'dark'>('light');
export const glassSupported = writable(false);
export const reduceTransparency = writable(false);

export async function initSystemAppearance(): Promise<void> {
  // Tauri APIs are only available in the main webview, not in iframes or plain
  // browsers.  Guard before touching any Tauri API so that the presenter and
  // caption pages (which are accessed without Tauri context) don't throw.
  const theme = await watchHostTheme((next) => systemTheme.set(next));
  if (theme === null) return;
  systemTheme.set(theme);

  const mq = window.matchMedia('(prefers-reduced-transparency: reduce)');
  reduceTransparency.set(mq.matches);
  mq.addEventListener('change', (e) => reduceTransparency.set(e.matches));

  glassSupported.set(await enableWindowGlass());
}
