/**
 * E2E tests for Recordings REST API.
 * Requires the server running at localhost:3737 with TAURI_TEST_TOKEN set.
 */

import { describe, it, expect, beforeAll, afterAll } from 'vitest';
import { apiClient } from '../helpers/client.js';

interface Recording {
  id: string;
  eventId: string;
  filePath: string;
  fileName: string;
  fileSize: number;
  durationSeconds: number;
  detectedAt: string;
  whitelisted: boolean;
  uploaded: boolean;
  uploads: string[];
  createdAt: string;
  updatedAt: string;
}

interface Event {
  id: string;
  title: string;
}

const isLive = !!process.env.TAURI_TEST_TOKEN;

describe.skipIf(!isLive)('Recordings REST API', () => {
  let eventId: string;
  let recordingId: string;

  beforeAll(async () => {
    const res = await apiClient.post<Event>('/api/events', {
      title: 'E2E Recordings Test Event',
      date_time: new Date(Date.now() + 86400000).toISOString(),
      speaker: 'Test',
      description: '',
    });
    expect(res.status).toBe(201);
    eventId = res.body.id;
  });

  afterAll(async () => {
    if (eventId) {
      await apiClient.delete(`/api/events/${eventId}`);
    }
  });

  it('GET /api/events/:id/recordings → 200 with array', async () => {
    const res = await apiClient.get<Recording[]>(`/api/events/${eventId}/recordings`);
    expect(res.status).toBe(200);
    expect(Array.isArray(res.body)).toBe(true);
  });

  it('POST /api/events/:id/recordings → 201', async () => {
    const res = await apiClient.post<Recording>(`/api/events/${eventId}/recordings`, {
      file_path: '/tmp/e2e-test.mp4',
      file_name: 'e2e-test.mp4',
      file_size: 1024,
      duration_seconds: 60.0,
    });
    expect(res.status).toBe(201);
    expect(res.body).toHaveProperty('id');
    recordingId = res.body.id;
  });

  it('GET /api/recordings → 200 with array (all recordings)', async () => {
    const res = await apiClient.get<Recording[]>('/api/recordings');
    expect(res.status).toBe(200);
    expect(Array.isArray(res.body)).toBe(true);
  });

  it('GET /api/recordings/untracked → 200 with array', async () => {
    const res = await apiClient.get<Recording[]>('/api/recordings/untracked');
    expect(res.status).toBe(200);
    expect(Array.isArray(res.body)).toBe(true);
  });

  it('DELETE /api/events/:id/recordings/:recordingId → 204', async () => {
    expect(recordingId).toBeDefined();
    const res = await apiClient.delete(`/api/events/${eventId}/recordings/${recordingId}`);
    expect(res.status).toBe(204);
  });
});
