/**
 * E2E tests for Activities REST API.
 */

import { describe, it, expect, beforeAll, afterAll } from 'vitest';
import { apiClient } from '../helpers/client.js';

interface Activity {
  id: string;
  eventId: string;
  activityType: string;
  message: string | null;
  createdAt: string;
}

interface Event {
  id: string;
}

const isLive = !!process.env.TAURI_TEST_TOKEN;

describe.skipIf(!isLive)('Activities REST API', () => {
  let eventId: string;
  let activityId: string;

  beforeAll(async () => {
    const res = await apiClient.post<Event>('/api/events', {
      title: 'E2E Activities Test Event',
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

  it('GET /api/events/:id/activities → 200 with array', async () => {
    const res = await apiClient.get<Activity[]>(`/api/events/${eventId}/activities`);
    expect(res.status).toBe(200);
    expect(Array.isArray(res.body)).toBe(true);
  });

  it('POST /api/events/:id/activities → 201', async () => {
    const res = await apiClient.post<Activity>(`/api/events/${eventId}/activities`, {
      activity_type: 'note',
      message: 'E2E test note',
    });
    expect(res.status).toBe(201);
    expect(res.body).toHaveProperty('id');
    expect(res.body.activityType).toBe('note');
    activityId = res.body.id;
  });

  it('DELETE /api/events/:id/activities/:activityId → 204', async () => {
    expect(activityId).toBeDefined();
    const res = await apiClient.delete(`/api/events/${eventId}/activities/${activityId}`);
    expect(res.status).toBe(204);
  });
});
