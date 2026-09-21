export type Scheme = 'light' | 'dark' | 'auto';

const KEY = 'sanctum-scheme';

function systemDark(): boolean {
  return typeof window !== 'undefined' && window.matchMedia('(prefers-color-scheme: dark)').matches;
}

function resolve(s: Scheme): 'light' | 'dark' {
  return s === 'auto' ? (systemDark() ? 'dark' : 'light') : s;
}

function apply(s: Scheme): void {
  document.documentElement.setAttribute('data-scheme', resolve(s));
}

let current = $state<Scheme>('auto');

/** The chosen setting (light/dark/auto), reactive. */
export function scheme(): Scheme {
  return current;
}

export function setScheme(s: Scheme): void {
  current = s;
  try {
    localStorage.setItem(KEY, s);
  } catch {
    /* storage unavailable */
  }
  apply(s);
}

/** Read the persisted choice, apply it, and keep `auto` in sync with the OS. */
export function initScheme(): void {
  current = (localStorage.getItem(KEY) as Scheme | null) ?? 'auto';
  apply(current);
  window
    .matchMedia('(prefers-color-scheme: dark)')
    .addEventListener('change', () => current === 'auto' && apply(current));
}
