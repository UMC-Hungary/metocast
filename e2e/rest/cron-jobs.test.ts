/**
 * E2E tests for Cron Jobs REST API.
 */

import { describe, it, expect } from 'vitest';
import { apiClient } from '../helpers/client.js';

interface CronJob {
  id: string;
  name: string;
  cronExpression: string;
  enabled: boolean;
  pullYoutube: boolean;
  autoUpload: boolean;
  createdAt: string;
  updatedAt: string;
}

const isLive = !!process.env.TAURI_TEST_TOKEN;

describe.skipIf(!isLive)('Cron Jobs REST API', () => {
  let cronJobId: string;

  it('GET /api/cron-jobs → 200 with array', async () => {
    const res = await apiClient.get<CronJob[]>('/api/cron-jobs');
    expect(res.status).toBe(200);
    expect(Array.isArray(res.body)).toBe(true);
  });

  it('POST /api/cron-jobs → 201', async () => {
    const res = await apiClient.post<CronJob>('/api/cron-jobs', {
      name: 'E2E Test Job',
      cronExpression: '0 0 9 * * 1',
      enabled: false,
      pullYoutube: false,
      autoUpload: false,
    });
    expect(res.status).toBe(201);
    expect(res.body).toHaveProperty('id');
    cronJobId = res.body.id;
  });

  it('PUT /api/cron-jobs/:id → 200 with updated job', async () => {
    expect(cronJobId).toBeDefined();
    const res = await apiClient.put<CronJob>(`/api/cron-jobs/${cronJobId}`, {
      name: 'E2E Test Job (updated)',
      cronExpression: '0 0 10 * * 1',
      enabled: true,
      pullYoutube: true,
      autoUpload: false,
    });
    expect(res.status).toBe(200);
    expect(res.body.name).toBe('E2E Test Job (updated)');
  });

  it('DELETE /api/cron-jobs/:id → 204', async () => {
    expect(cronJobId).toBeDefined();
    const res = await apiClient.delete(`/api/cron-jobs/${cronJobId}`);
    expect(res.status).toBe(204);
  });
});
