import { addMessages, init, locale } from 'svelte-i18n';
import { loadHostStore } from '@metocast/core-client';
import { browser } from '$app/environment';
import en from '@metocast/core-client/locales/en.json';
import hu from '@metocast/core-client/locales/hu.json';

// Use addMessages (synchronous) so $_() works during SSR
addMessages('en', en);
addMessages('hu', hu);

function getInitialLocale(): string {
  if (browser) {
    const saved = localStorage.getItem('locale');
    if (saved) return saved;
  }
  return 'en';
}

init({
  fallbackLocale: 'en',
  initialLocale: getInitialLocale(),
});

export async function loadSavedLocale(): Promise<void> {
  if (!browser) return;

  try {
    const store = await loadHostStore('settings.json');
    const savedLocale = (await store?.get('locale')) as string | null | undefined;
    if (savedLocale) {
      locale.set(savedLocale);
    }
  } catch (error) {
    console.warn('Failed to load saved locale from Tauri store:', error);
  }
}

export async function setLocale(newLocale: string): Promise<void> {
  locale.set(newLocale);
  if (!browser) return;

  try {
    localStorage.setItem('locale', newLocale);
    const store = await loadHostStore('settings.json');
    if (store) {
      await store.set('locale', newLocale);
      await store.save();
    }
  } catch (error) {
    console.warn('Failed to save locale:', error);
  }
}

export const availableLocales = [
  { code: 'en', name: 'English', flag: '🇬🇧' },
  { code: 'hu', name: 'Magyar', flag: '🇭🇺' },
];
