/**
 * E2E tests for PPT REST API.
 */

import { describe, it, expect, beforeAll, afterAll } from 'vitest';
import { mkdtempSync, renameSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { apiClient } from '../helpers/client.js';

interface PptFolder {
  id: string;
  path: string;
  name: string;
  sortOrder: number;
}

interface HighlightPart {
  text: string;
  hit: boolean;
}

interface PptFile {
  id: string;
  name: string;
  path: string;
  folderId: string;
  title: string;
  number: number | null;
  matchedIn: 'number' | 'title' | 'text' | null;
  titleParts: HighlightPart[];
  snippetParts: HighlightPart[];
}

const isLive = !!process.env.TAURI_TEST_TOKEN;

const search = (filter: string, extra: Record<string, string> = {}) =>
  apiClient.get<{ success: boolean; data: PptFile[] }>('/api/ppt/files', {
    filter,
    limit: '50',
    ...extra,
  });

const hits = (parts: HighlightPart[]) => parts.filter((p) => p.hit).map((p) => p.text);

describe.skipIf(!isLive)('PPT REST API', () => {
  let folderId: string;
  let folderPath: string;

  beforeAll(async () => {
    folderPath = mkdtempSync(join(tmpdir(), 'e2e-ppt-test-'));
    // Real decks, written by the core's own song deck writer. It drops punctuation from file
    // names, so each is renamed to the "Title (number)" form hymn decks use.
    await apiClient.put('/api/settings/song-slide-folder', { path: folderPath });
    for (const [title, lyrics] of [
      ['Ó, hála az Úrnak (47)', 'Zengjen ma minden szív\n\nKegyelme örökkévaló'],
      ['Erős vár a mi Istenünk (390)', 'Ő a mi pajzsunk és fegyverünk'],
    ] as const) {
      const res = await apiClient.post<{ filePath: string }>('/api/ppt/song', { title, lyrics });
      expect(res.status).toBe(200);
      renameSync(res.body.filePath, join(folderPath, `${title}.pptx`));
    }
  });

  afterAll(async () => {
    await apiClient.put('/api/settings/song-slide-folder', { path: '' });
    rmSync(folderPath, { recursive: true, force: true });
  });

  it('GET /api/ppt/folders → 200', async () => {
    const res = await apiClient.get<{ success: boolean; data: PptFolder[] }>('/api/ppt/folders');
    expect(res.status).toBe(200);
    expect(res.body).toHaveProperty('success', true);
  });

  it('POST /api/ppt/folders → 201', async () => {
    const res = await apiClient.post<{ success: boolean; data: PptFolder }>('/api/ppt/folders', {
      path: folderPath,
      name: 'E2E Test Folder',
    });
    expect(res.status).toBe(201);
    expect(res.body.success).toBe(true);
    folderId = res.body.data.id;
  });

  it('finds a deck by its hymn number first', async () => {
    // Adding the folder indexes it in the background; a search meanwhile answers from the
    // index as it is, so wait for that first sync.
    let res = await search('47', { folderId });
    for (let i = 0; i < 50 && res.body.data.length === 0; i++) {
      await new Promise((resolve) => setTimeout(resolve, 100));
      res = await search('47', { folderId });
    }
    expect(res.status).toBe(200);
    const [first] = res.body.data;
    expect(first?.title).toBe('Ó, hála az Úrnak (47)');
    expect(first?.number).toBe(47);
    expect(first?.matchedIn).toBe('number');
  });

  it('finds a title without its accents and marks the accented word', async () => {
    const res = await search('hala urnak', { folderId });
    const [first] = res.body.data;
    expect(first?.matchedIn).toBe('title');
    expect(hits(first?.titleParts ?? [])).toEqual(['hála', 'Úrnak']);
  });

  it('finds slide text by a word prefix and returns a snippet', async () => {
    const res = await search('pajzs', { folderId });
    const [first] = res.body.data;
    expect(first?.title).toBe('Erős vár a mi Istenünk (390)');
    expect(first?.matchedIn).toBe('text');
    expect(hits(first?.snippetParts ?? [])).toEqual(['pajzsunk']);
  });

  it('lists every deck by title for an empty filter', async () => {
    const res = await search('', { folderId });
    expect(res.body.data.map((f) => f.title)).toEqual([
      'Erős vár a mi Istenünk (390)',
      'Ó, hála az Úrnak (47)',
    ]);
    expect(res.body.data.every((f) => f.matchedIn === null)).toBe(true);
  });

  it('keeps 5 results by default', async () => {
    const res = await apiClient.get<{ data: PptFile[] }>('/api/ppt/files');
    expect(res.status).toBe(200);
    expect(res.body.data.length).toBeLessThanOrEqual(5);
  });

  it('DELETE /api/ppt/folders/:id → 200 and its decks leave the index', async () => {
    expect(folderId).toBeDefined();
    const res = await apiClient.delete(`/api/ppt/folders/${folderId}`);
    expect(res.status).toBe(200);
    const after = await search('', { folderId });
    expect(after.body.data).toEqual([]);
  });

  it('GET /api/ppt/files → 200 for punctuation only', async () => {
    const res = await search('&|!(');
    expect(res.status).toBe(200);
    expect(res.body.data).toEqual([]);
  });
});
