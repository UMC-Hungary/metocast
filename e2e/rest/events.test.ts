/**
 * E2E tests for Events REST API.
 * Requires the server running at localhost:3737 with TAURI_TEST_TOKEN set.
 *
 * Run: pnpm test:e2e
 */

import { describe, it, expect } from 'vitest';
import { apiClient } from '../helpers/client.js';

interface Event {
  id: string;
  title: string;
  dateTime: string;
  speaker: string;
  description: string;
  autoUploadEnabled: boolean;
  connections: string[];
  bibleReferences: string[];
  createdAt: string;
  updatedAt: string;
}

interface EventSummary {
  id: string;
  title: string;
  dateTime: string;
  speaker: string;
  recordingCount: number;
  isCompleted: boolean;
  createdAt: string;
  updatedAt: string;
}

const isLive = !!process.env.TAURI_TEST_TOKEN;

describe.skipIf(!isLive)('Events REST API', () => {
  let createdEventId: string;

  it('GET /api/events → 200 with array', async () => {
    const res = await apiClient.get<EventSummary[]>('/api/events');
    expect(res.status).toBe(200);
    expect(Array.isArray(res.body)).toBe(true);
  });

  it('POST /api/events → 201 with event', async () => {
    const res = await apiClient.post<Event>('/api/events', {
      title: 'E2E Test Event',
      date_time: new Date(Date.now() + 86400000).toISOString(),
      speaker: 'Test Speaker',
      description: 'Created by E2E test',
    });
    expect(res.status).toBe(201);
    expect(res.body).toHaveProperty('id');
    expect(res.body.title).toBe('E2E Test Event');
    createdEventId = res.body.id;
  });

  it('GET /api/events/:id → 200 with event', async () => {
    expect(createdEventId).toBeDefined();
    const res = await apiClient.get<Event>(`/api/events/${createdEventId}`);
    expect(res.status).toBe(200);
    expect(res.body.id).toBe(createdEventId);
  });

  it('PUT /api/events/:id → 200 with updated event', async () => {
    expect(createdEventId).toBeDefined();
    const res = await apiClient.put<Event>(`/api/events/${createdEventId}`, {
      title: 'E2E Test Event (updated)',
      date_time: new Date(Date.now() + 86400000).toISOString(),
      speaker: 'Updated Speaker',
      description: 'Updated by E2E test',
    });
    expect(res.status).toBe(200);
    expect(res.body.title).toBe('E2E Test Event (updated)');
  });

  it('DELETE /api/events/:id → 204', async () => {
    expect(createdEventId).toBeDefined();
    const res = await apiClient.delete(`/api/events/${createdEventId}`);
    expect(res.status).toBe(204);
  });
});
