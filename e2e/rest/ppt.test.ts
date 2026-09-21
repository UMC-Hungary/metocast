/**
 * E2E tests for PPT REST API.
 */

import { describe, it, expect, beforeAll, afterAll } from 'vitest';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { apiClient } from '../helpers/client.js';

interface PptFolder {
  id: string;
  path: string;
  name: string;
  sortOrder: number;
}

const isLive = !!process.env.TAURI_TEST_TOKEN;

describe.skipIf(!isLive)('PPT REST API', () => {
  let folderId: string;
  let folderPath: string;

  beforeAll(() => {
    folderPath = mkdtempSync(join(tmpdir(), 'e2e-ppt-test-'));
  });

  afterAll(() => {
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

  it('DELETE /api/ppt/folders/:id → 200', async () => {
    expect(folderId).toBeDefined();
    const res = await apiClient.delete(`/api/ppt/folders/${folderId}`);
    expect(res.status).toBe(200);
  });

  it('GET /api/ppt/files → 200', async () => {
    const res = await apiClient.get('/api/ppt/files', { filter: 'test' });
    expect(res.status).toBe(200);
  });
});
