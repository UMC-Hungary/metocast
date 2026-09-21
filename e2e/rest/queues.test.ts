/**
 * E2E tests for the job-queue REST API.
 *
 * Creating a future event enqueues a `youtube.upsert` job, which exercises the
 * migration, the dedup index and the claim/redrive endpoints end to end.
 */

import { describe, it, expect, beforeAll, afterAll } from 'vitest';
import { apiClient } from '../helpers/client.js';
import { WsTestClient } from '../helpers/ws-client.js';

const isLive = !!process.env.TAURI_TEST_TOKEN;

interface Job {
  id: string;
  jobType: string;
  status: string;
  attempts: number;
  maxAttempts: number;
  lastError: string | null;
}

interface QueueSummary {
  queue: string;
  pending: number;
  processing: number;
  succeeded: number;
  dead: number;
}

describe.skipIf(!isLive)('Queues REST API', () => {
  let ws: WsTestClient;

  beforeAll(async () => {
    ws = new WsTestClient();
    await ws.waitForConnect();
  });

  afterAll(() => ws.close());

  it('GET /api/queues → per-queue depth by status', async () => {
    const res = await apiClient.get<QueueSummary[]>('/api/queues');
    expect(res.status).toBe(200);
    expect(Array.isArray(res.body)).toBe(true);
  });

  it('parks the upsert job while YouTube is logged out, coalescing repeat edits', async () => {
    const dateTime = new Date(Date.now() + 86_400_000).toISOString();
    const created = await apiClient.post<{ id: string }>('/api/events', {
      title: 'Queue test event',
      date_time: dateTime,
    });
    expect(created.status).toBe(201);
    const eventId = created.body.id;

    for (const title of ['Queue test edit 1', 'Queue test edit 2']) {
      await apiClient.put(`/api/events/${eventId}`, { title, date_time: dateTime });
    }

    // The test server has no YouTube credential, so the worker must not claim
    // the job: it waits in the queue for the user to log back in. All three
    // edits share a dedup_key, and the partial unique index makes a second
    // pending row for that key impossible — so this is exactly the one job the
    // edits coalesced onto, however late the last enqueue lands.
    const jobs = await pendingJobs(eventId);
    expect(jobs).toHaveLength(1);
    expect(jobs[0]?.attempts).toBe(0);
    expect(jobs[0]?.lastError).toBeNull();

    const retry = await apiClient.post(`/api/jobs/${jobs[0]?.id}/retry`);
    expect(retry.status).toBe(204);

    const purge = await apiClient.delete(`/api/jobs/${jobs[0]?.id}`);
    expect(purge.status).toBe(204);

    await apiClient.delete(`/api/events/${eventId}`);
  });

  it('POST /api/jobs/{id}/retry on an unknown id → 404', async () => {
    const res = await apiClient.post('/api/jobs/00000000-0000-0000-0000-000000000000/retry');
    expect(res.status).toBe(404);
  });

  /**
   * Pending upsert jobs for one event.
   *
   * `queue.stats` carries depth counts rather than job rows, so it says "the
   * queue moved", not "your job landed" — this waits on that push and re-reads
   * the list, the same way the dashboard page refetches on every stats message.
   * No timers: an enqueue that never arrives fails via vitest's testTimeout.
   */
  async function pendingJobs(eventId: string): Promise<Job[]> {
    for (;;) {
      const res = await apiClient.get<Job[]>('/api/queues/platform_sync/jobs', {
        status: 'pending',
      });
      const jobs = res.body.filter(
        (j) => j.jobType === 'youtube.upsert' && JSON.stringify(j).includes(eventId),
      );
      if (jobs.length > 0) return jobs;
      await ws.waitForMessage('queue.stats');
    }
  }
});
