import { z } from 'zod';
import { coreConfig } from '../config.js';
import { PptFileSchema } from '../schemas/ws-messages.js';
import type { PptFolder, PptFile, KeynoteStatus } from '../schemas/ws-messages.js';
import { apiFetch as typedApiFetch } from './client.js';

const SongPresentationResultSchema = z.object({
  filePath: z.string(),
  slideCount: z.number().int().positive(),
});

export type SongPresentationResult = z.infer<typeof SongPresentationResultSchema>;

function getBaseUrl(): string {
  const { mode, serverPort, serverUrl } = coreConfig();
  if (mode === 'server') {
    return `http://localhost:${serverPort}`;
  }
  return serverUrl;
}

function getHeaders(): Record<string, string> {
  return {
    'Content-Type': 'application/json',
    Authorization: `Bearer ${coreConfig().authToken}`,
  };
}

async function apiFetch<T>(
  path: string,
  options?: RequestInit,
): Promise<{ success: boolean; data?: T; error?: string }> {
  try {
    const res = await fetch(`${getBaseUrl()}${path}`, {
      ...options,
      headers: { ...getHeaders(), ...((options?.headers as Record<string, string>) ?? {}) },
    });
    return (await res.json()) as { success: boolean; data?: T; error?: string };
  } catch (e) {
    return { success: false, error: e instanceof Error ? e.message : 'Network error' };
  }
}

// ── Folder management ─────────────────────────────────────────────────────────

export async function listFolders(): Promise<PptFolder[]> {
  const result = await apiFetch<PptFolder[]>('/api/ppt/folders');
  return result.success && result.data ? result.data : [];
}

/** Rejects with the core's message when the path is not a folder on the core's machine. */
export async function addFolder(path: string, name: string): Promise<PptFolder> {
  const result = await apiFetch<PptFolder>('/api/ppt/folders', {
    method: 'POST',
    body: JSON.stringify({ path, name }),
  });
  if (!result.success || !result.data) throw new Error(result.error ?? 'Failed to add folder');
  return result.data;
}

export async function removeFolder(id: string): Promise<boolean> {
  const result = await apiFetch(`/api/ppt/folders/${id}`, { method: 'DELETE' });
  return result.success;
}

// ── File search ───────────────────────────────────────────────────────────────

export interface SearchFilesOptions {
  /** Only this folder's decks. */
  folderId?: string;
  /** How many decks to return; the core answers 5 when omitted, at most 200. */
  limit?: number;
}

/** Decks by title, hymn number or slide text, accents ignored, best match first. */
export async function searchFiles(
  filter: string,
  options: SearchFilesOptions = {},
): Promise<PptFile[]> {
  const params = new URLSearchParams();
  if (filter) params.set('filter', filter);
  if (options.folderId) params.set('folderId', options.folderId);
  if (options.limit !== undefined) params.set('limit', String(options.limit));
  const query = params.size > 0 ? `?${params}` : '';
  const result = await apiFetch<unknown>(`/api/ppt/files${query}`);
  const files = z.array(PptFileSchema).safeParse(result.data);
  return result.success && files.success ? files.data : [];
}

export function createSongPresentation(
  title: string,
  lyrics: string,
): Promise<SongPresentationResult> {
  return typedApiFetch('/api/ppt/song', SongPresentationResultSchema, {
    method: 'POST',
    body: { title, lyrics },
  });
}

// ── Keynote control ───────────────────────────────────────────────────────────

export async function keynoteStatus(): Promise<KeynoteStatus | null> {
  const result = await apiFetch<KeynoteStatus>('/api/keynote/status');
  return result.success && result.data ? result.data : null;
}

export async function openFile(filePath: string): Promise<boolean> {
  const result = await apiFetch('/api/keynote/open', {
    method: 'POST',
    body: JSON.stringify({ filePath }),
  });
  return result.success;
}

export async function keynoteNext(): Promise<void> {
  await apiFetch('/api/keynote/next', { method: 'POST' });
}

export async function keynotePrev(): Promise<void> {
  await apiFetch('/api/keynote/prev', { method: 'POST' });
}

export async function keynoteFirst(): Promise<void> {
  await apiFetch('/api/keynote/first', { method: 'POST' });
}

export async function keynoteLast(): Promise<void> {
  await apiFetch('/api/keynote/last', { method: 'POST' });
}

export async function keynoteGoto(slide: number): Promise<void> {
  await apiFetch('/api/keynote/goto', {
    method: 'POST',
    body: JSON.stringify({ slide }),
  });
}

export async function keynoteStart(): Promise<void> {
  await apiFetch('/api/keynote/start', { method: 'POST' });
}

export async function keynoteStop(): Promise<void> {
  await apiFetch('/api/keynote/stop', { method: 'POST' });
}

export async function keynoteCloseAll(): Promise<void> {
  await apiFetch('/api/keynote/close_all', { method: 'POST' });
}

/** The welcome screen a text Order of Service step shows: greeting, blessing, church, photo interval. */
export const WelcomeSettingsSchema = z.object({
  title: z.string(),
  subtitle: z.string(),
  churchName: z.string(),
  intervalSeconds: z.number().int().min(5).max(3600),
  hasLogo: z.boolean(),
  /** Photos (1–12) replaced by an upload. */
  replacedPhotos: z.array(z.number().int().min(1).max(12)),
});

export type WelcomeSettings = z.infer<typeof WelcomeSettingsSchema>;

export function getWelcomeSettings(): Promise<WelcomeSettings> {
  return typedApiFetch('/api/presenter/welcome', WelcomeSettingsSchema);
}

/** `hasLogo` and `replacedPhotos` come from the uploads and are not sent. */
export function setWelcomeSettings(
  settings: Pick<WelcomeSettings, 'title' | 'subtitle' | 'churchName' | 'intervalSeconds'>,
): Promise<WelcomeSettings> {
  return typedApiFetch('/api/presenter/welcome', WelcomeSettingsSchema, {
    method: 'PUT',
    body: settings,
  });
}
