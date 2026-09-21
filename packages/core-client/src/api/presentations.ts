import { z } from 'zod';
import { coreConfig } from '../config.js';
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

export async function searchFiles(filter: string): Promise<PptFile[]> {
  const params = filter ? `?filter=${encodeURIComponent(filter)}` : '';
  const result = await apiFetch<PptFile[]>(`/api/ppt/files${params}`);
  return result.success && result.data ? result.data : [];
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
