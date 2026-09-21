/**
 * E2E tests for Uploads REST API.
 */

import { describe, it, expect } from 'vitest';
import { apiClient } from '../helpers/client.js';

const isLive = !!process.env.TAURI_TEST_TOKEN;

describe.skipIf(!isLive)('Uploads REST API', () => {
  it('POST /api/uploads/trigger → 204', async () => {
    const res = await apiClient.post('/api/uploads/trigger');
    expect(res.status).toBe(204);
  });
});
