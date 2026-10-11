/**
 * E2E tests for the media-processing REST API.
 *
 * The test server has no camera clip or recording, so these cover what every host can check:
 * the stored defaults, and that a request naming a file the server must not read
 * is refused before any tool runs.
 */

import { describe, it, expect } from 'vitest';
import { apiClient } from '../helpers/client.js';

const isLive = !!process.env.TAURI_TEST_TOKEN;

interface MediaConfig {
  enabled: boolean;
  gamma: string;
  lutMode: string;
  aspect: string;
  bitrateMbps: number;
  audioTargetLufs: string;
}

describe.skipIf(!isLive)('Media REST API', () => {
  it('GET /api/connectors/media/config → the camera look by default', async () => {
    const res = await apiClient.get<MediaConfig>('/api/connectors/media/config');
    expect(res.status).toBe(200);
    expect(res.body.gamma).toBe('Blackmagic Design Film');
    expect(res.body.lutMode).toBe('embedded');
    expect(res.body.aspect).toBe('16:9');
    expect(res.body.bitrateMbps).toBe(100);
    expect(res.body.audioTargetLufs).toBe('-14');
  });

  it('PUT /api/connectors/media/config refuses a loudness target out of range', async () => {
    const res = await apiClient.put('/api/connectors/media/config', { audioTargetLufs: '-3' });
    expect(res.status).toBe(400);
  });

  it('PUT /api/connectors/media/config refuses text that is not a number', async () => {
    const res = await apiClient.put('/api/connectors/media/config', { tint: 'warm' });
    expect(res.status).toBe(400);
  });

  it('POST /api/media/probe refuses a relative path', async () => {
    const res = await apiClient.post('/api/media/probe', { path: 'clip.braw' });
    expect(res.status).toBe(400);
  });

  it('POST /api/media/probe refuses a file that is not video', async () => {
    const res = await apiClient.post('/api/media/probe', { path: '/etc/hosts' });
    expect(res.status).toBe(400);
  });

  it('POST /api/media/probe on a missing file → 404', async () => {
    const res = await apiClient.post('/api/media/probe', { path: '/nowhere/clip.braw' });
    expect(res.status).toBe(404);
  });

  it('POST /api/media/preview on a missing file → 404', async () => {
    const res = await apiClient.post('/api/media/preview', { path: '/nowhere/clip.braw', frame: 0 });
    expect(res.status).toBe(404);
  });

  it('POST /api/media/renders on a missing file → 404', async () => {
    const res = await apiClient.post('/api/media/renders', { path: '/nowhere/clip.braw' });
    expect(res.status).toBe(404);
  });

  it('POST /api/media/audio/probe refuses an empty list', async () => {
    const res = await apiClient.post('/api/media/audio/probe', { paths: [] });
    expect(res.status).toBe(400);
  });

  it('POST /api/media/audio/probe refuses a file that is not audio', async () => {
    const res = await apiClient.post('/api/media/audio/probe', { paths: ['/etc/hosts'] });
    expect(res.status).toBe(400);
  });

  it('POST /api/media/audio on a missing file → 404', async () => {
    const res = await apiClient.post('/api/media/audio', {
      paths: ['/nowhere/00015_Wireless_PRO.WAV'],
    });
    expect(res.status).toBe(404);
  });

  it('POST /api/media/programs/probe refuses a relative video path', async () => {
    const res = await apiClient.post('/api/media/programs/probe', { videoPath: 'clip.braw' });
    expect(res.status).toBe(400);
  });

  it('POST /api/media/programs on a missing video → 404', async () => {
    const res = await apiClient.post('/api/media/programs', {
      videoPath: '/nowhere/clip.braw',
      audioPaths: ['/nowhere/00015_Wireless_PRO.WAV'],
    });
    expect(res.status).toBe(404);
  });

  it('POST /api/media/programs/{id}/nudge refuses zero frames', async () => {
    const res = await apiClient.post(
      '/api/media/programs/00000000-0000-0000-0000-000000000000/nudge',
      { frames: 0 },
    );
    expect(res.status).toBe(400);
  });

  it('POST /api/media/programs/{id}/nudge on an unknown program → 404', async () => {
    const res = await apiClient.post(
      '/api/media/programs/00000000-0000-0000-0000-000000000000/nudge',
      { frames: 1 },
    );
    expect(res.status).toBe(404);
  });

  it('POST /api/media/renders/{id}/cancel on an unknown id → 404', async () => {
    const res = await apiClient.post(
      '/api/media/renders/00000000-0000-0000-0000-000000000000/cancel',
    );
    expect(res.status).toBe(404);
  });
});
