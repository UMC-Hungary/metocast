/**
 * E2E tests for the Presenter REST API.
 */

import { describe, it, expect } from 'vitest';
import { apiClient } from '../helpers/client.js';

const isLive = !!process.env.TAURI_TEST_TOKEN;

describe.skipIf(!isLive)('Presenter REST API', () => {
  it('POST /api/presenter/parse → 422 for non-existent file', async () => {
    const res = await apiClient.post<{ success: boolean; error: string }>('/api/presenter/parse', {
      filePath: '/tmp/e2e-nonexistent-file.pptx',
    });
    expect(res.status).toBe(422);
    expect(res.body.success).toBe(false);
    expect(typeof res.body.error).toBe('string');
  });

  it('POST /api/presenter/parse → 422 for non-pptx file', async () => {
    const res = await apiClient.post<{ success: boolean; error: string }>('/api/presenter/parse', {
      filePath: '/etc/hosts',
    });
    expect(res.status).toBe(422);
    expect(res.body.success).toBe(false);
  });
});
