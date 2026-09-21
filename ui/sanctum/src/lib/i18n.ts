import { addMessages, init, locale } from 'svelte-i18n';
import en from './locales/en.json';
import hu from './locales/hu.json';

// Sanctum's OWN catalogues — deliberately separate from ui/classic's list. Added
// synchronously so $_() resolves during the static prerender, not just after hydration.
addMessages('en', en);
addMessages('hu', hu);

const KEY = 'sanctum-locale';

function initialLocale(): string {
  try {
    const saved = localStorage.getItem(KEY);
    if (saved) return saved;
  } catch {
    /* storage unavailable */
  }
  return 'en';
}

init({ fallbackLocale: 'en', initialLocale: initialLocale() });

export const locales = [
  { code: 'en', label: 'English' },
  { code: 'hu', label: 'Magyar' },
];

export function setLocale(code: string): void {
  locale.set(code);
  try {
    localStorage.setItem(KEY, code);
  } catch {
    /* storage unavailable */
  }
}
