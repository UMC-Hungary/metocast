import { loadHostStore } from '../host/index.js';

const browser = typeof window !== 'undefined';

export type CaptionType = 'caption' | 'preview';
export type Resolution = '1080p' | '4k';

export interface CaptionSettings {
  type: CaptionType;
  title: string;
  boldText: string;
  lightText: string;
  color: string;
  showLogo: boolean;
  logoAlt: string;
  svgLogo: string;
  resolution: Resolution;
}

export const RESOLUTION_DIMENSIONS: Record<Resolution, { width: number; height: number }> = {
  '1080p': { width: 1920, height: 1080 },
  '4k': { width: 3840, height: 2160 },
};

export function getCaptionHeight(type: CaptionType, resolution: Resolution): number {
  if (type === 'caption') {
    return resolution === '4k' ? 300 : 150;
  }
  return RESOLUTION_DIMENSIONS[resolution].height;
}

const STORE_NAME = 'caption-settings.json';

const DEFAULT_SETTINGS: CaptionSettings = {
  type: 'caption',
  title: '',
  boldText: '',
  lightText: '',
  color: 'black',
  showLogo: true,
  logoAlt: '',
  svgLogo: '',
  resolution: '1080p',
};

function getDefaultSettings(): CaptionSettings {
  return { ...DEFAULT_SETTINGS };
}

async function getSettings(): Promise<CaptionSettings> {
  if (!browser) return getDefaultSettings();

  try {
    const store = await loadHostStore(STORE_NAME);
    if (store) {
      const [type, title, boldText, lightText, color, showLogo, logoAlt, svgLogo, resolution] =
        await Promise.all([
          store.get<CaptionType>('type'),
          store.get<string>('title'),
          store.get<string>('boldText'),
          store.get<string>('lightText'),
          store.get<string>('color'),
          store.get<boolean>('showLogo'),
          store.get<string>('logoAlt'),
          store.get<string>('svgLogo'),
          store.get<Resolution>('resolution'),
        ]);

      return {
        type: type ?? DEFAULT_SETTINGS.type,
        title: title ?? DEFAULT_SETTINGS.title,
        boldText: boldText ?? DEFAULT_SETTINGS.boldText,
        lightText: lightText ?? DEFAULT_SETTINGS.lightText,
        color: color ?? DEFAULT_SETTINGS.color,
        showLogo: showLogo ?? DEFAULT_SETTINGS.showLogo,
        logoAlt: logoAlt ?? DEFAULT_SETTINGS.logoAlt,
        svgLogo: svgLogo ?? DEFAULT_SETTINGS.svgLogo,
        resolution: resolution ?? DEFAULT_SETTINGS.resolution,
      };
    }
  } catch (error) {
    console.warn('Failed to load caption settings from Tauri store:', error);
  }

  // Browser localStorage fallback
  try {
    const raw = localStorage.getItem(STORE_NAME);
    if (raw) {
      return { ...DEFAULT_SETTINGS, ...(JSON.parse(raw) as Partial<CaptionSettings>) };
    }
  } catch {
    // ignore parse errors
  }
  return getDefaultSettings();
}

async function saveSettings(settings: Partial<CaptionSettings>): Promise<void> {
  if (!browser) return;

  const merged = { ...(await getSettings()), ...settings };

  try {
    const store = await loadHostStore(STORE_NAME);
    if (store) {
      const keys = Object.keys(merged) as Array<keyof CaptionSettings>;
      await Promise.all(keys.map((key) => store.set(key, merged[key])));
      await store.save();
      return;
    }
  } catch (error) {
    console.warn('Failed to save caption settings to Tauri store:', error);
  }

  // Browser localStorage fallback
  localStorage.setItem(STORE_NAME, JSON.stringify(merged));
}

export const captionSettingsStore = { getDefaultSettings, getSettings, saveSettings };
