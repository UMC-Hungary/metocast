/**
 * E2E tests for the Bible and song output folder settings. The core stores the paths
 * because the core is what writes the decks, so it — not the UI — is what has
 * to reject a folder that is not there.
 */

import { describe, it, expect, afterAll } from 'vitest';
import { mkdtemp, readdir, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { apiClient } from '../helpers/client.js';

interface SlideFolder {
  path: string;
}

const isLive = !!process.env.TAURI_TEST_TOKEN;

describe.skipIf(!isLive)('Slide folder REST API', () => {
  afterAll(async () => {
    await apiClient.put('/api/settings/slide-folder', { path: '' });
    await apiClient.put('/api/settings/song-slide-folder', { path: '' });
  });

  it('GET → 200 with an empty path when unset', async () => {
    await apiClient.put('/api/settings/slide-folder', { path: '' });
    const res = await apiClient.get<SlideFolder>('/api/settings/slide-folder');
    expect(res.status).toBe(200);
    expect(res.body.path).toBe('');
  });

  it('PUT round-trips a folder that exists', async () => {
    const dir = process.cwd();
    const put = await apiClient.put<SlideFolder>('/api/settings/slide-folder', { path: dir });
    expect(put.status).toBe(200);

    const get = await apiClient.get<SlideFolder>('/api/settings/slide-folder');
    expect(get.body.path).toBe(dir);
  });

  it('PUT → 400 for a folder that is not on the core’s machine', async () => {
    const res = await apiClient.put('/api/settings/slide-folder', {
      path: '/definitely/not/a/real/folder',
    });
    expect(res.status).toBe(400);
  });

  it('stores the song and Bible folders separately', async () => {
    const songDir = await mkdtemp(join(tmpdir(), 'metocast-song-folder-'));
    await apiClient.put('/api/settings/slide-folder', { path: process.cwd() });
    await apiClient.put('/api/settings/song-slide-folder', { path: songDir });

    const bible = await apiClient.get<SlideFolder>('/api/settings/slide-folder');
    const song = await apiClient.get<SlideFolder>('/api/settings/song-slide-folder');
    expect(bible.body.path).toBe(process.cwd());
    expect(song.body.path).toBe(songDir);
    await rm(songDir, { recursive: true, force: true });
  });
});

describe.skipIf(!isLive)('Bible slide deck generation', () => {
  let dir = '';
  let eventId = '';

  afterAll(async () => {
    if (eventId) await apiClient.delete(`/api/events/${eventId}`);
    await apiClient.put('/api/settings/slide-folder', { path: '' });
    if (dir) await rm(dir, { recursive: true, force: true });
  });

  it('writes one deck per Bible reference into the configured folder', async () => {
    dir = await mkdtemp(join(tmpdir(), 'metocast-slides-'));
    await apiClient.put('/api/settings/slide-folder', { path: dir });

    const created = await apiClient.post<{ id: string }>('/api/events', {
      title: 'E2E Slide Event',
      date_time: new Date(Date.now() + 86400000).toISOString(),
      speaker: 'Test Speaker',
      description: '',
      bible_references: [
        {
          type: 'textus',
          reference: 'Jn 3,16',
          translation: 'RUF_v2',
          verses: [{ chapter: 3, verse: 16, text: 'Mert úgy szerette Isten a világot…' }],
        },
      ],
    });
    eventId = created.body.id;

    const res = await apiClient.post<{ files: string[] }>(`/api/events/${eventId}/slides`);
    expect(res.status).toBe(200);
    expect(res.body.files).toHaveLength(1);
    expect(await readdir(dir)).toEqual(['textus.pptx']);
  });

  it('POST → 400 when no folder is configured', async () => {
    await apiClient.put('/api/settings/slide-folder', { path: '' });
    const res = await apiClient.post(`/api/events/${eventId}/slides`);
    expect(res.status).toBe(400);
  });
});

describe.skipIf(!isLive)('Song slide deck generation', () => {
  let dir = '';

  afterAll(async () => {
    await apiClient.put('/api/settings/song-slide-folder', { path: '' });
    if (dir) await rm(dir, { recursive: true, force: true });
  });

  it('writes a safely named deck and paginates song sections', async () => {
    dir = await mkdtemp(join(tmpdir(), 'metocast-song-slides-'));
    await apiClient.put('/api/settings/song-slide-folder', { path: dir });

    const res = await apiClient.post<{ filePath: string; slideCount: number }>('/api/ppt/song', {
      title: '../Kegyelemből: ének',
      lyrics: 'egy\n\nkettő\n\n\nRefr.: hat\nhét',
    });

    expect(res.status).toBe(200);
    expect(res.body.slideCount).toBe(4);
    expect(res.body.filePath).toBe(join(dir, 'Kegyelemből ének.pptx'));
    expect(await readdir(dir)).toEqual(['Kegyelemből ének.pptx']);
  });
});
