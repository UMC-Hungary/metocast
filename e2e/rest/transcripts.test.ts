/**
 * E2E tests for the transcript library. A headless core has no RØDECaster, so it holds no
 * sessions; the stored-session paths are covered by the Rust tests.
 */

import { describe, it, expect } from 'vitest';
import { apiClient } from '../helpers/client.js';

const isLive = !!process.env.TAURI_TEST_TOKEN;
const MISSING = '00000000-0000-0000-0000-000000000000';

describe.skipIf(!isLive)('Transcript library REST API', () => {
  it('GET /api/transcripts → an array', async () => {
    const res = await apiClient.get<unknown>('/api/transcripts');
    expect(res.status).toBe(200);
    expect(Array.isArray(res.body)).toBe(true);
  });

  it('answers 404 for a transcript that does not exist', async () => {
    expect((await apiClient.get(`/api/transcripts/${MISSING}`)).status).toBe(404);
    expect(
      (await apiClient.get(`/api/transcripts/${MISSING}/download`, { format: 'srt' })).status,
    ).toBe(404);
    expect((await apiClient.delete(`/api/transcripts/${MISSING}`)).status).toBe(404);
  });

  it('refuses an unknown download format', async () => {
    const res = await apiClient.get(`/api/transcripts/${MISSING}/download`, { format: 'pdf' });
    expect(res.status).toBe(400);
  });
});
