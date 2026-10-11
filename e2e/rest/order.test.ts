/**
 * E2E tests for an event's Order of Service (`/api/events/{id}/order`).
 * Requires the isolated test server (see vitest.config.ts) with TAURI_TEST_TOKEN set.
 */

import { describe, it, expect, afterAll } from 'vitest';
import { apiClient } from '../helpers/client.js';

interface OrderItem {
  id: string;
  kind: 'song' | 'bible' | 'block';
  title: string;
  deckPath: string | null;
  bibleType: string | null;
  notes: string;
}

const isLive = !!process.env.TAURI_TEST_TOKEN;

describe.skipIf(!isLive)('Order of Service REST API', () => {
  let eventId = '';

  afterAll(async () => {
    if (eventId) await apiClient.delete(`/api/events/${eventId}`);
  });

  it('starts empty, stores steps in order and keeps ids across edits', async () => {
    const created = await apiClient.post<{ id: string }>('/api/events', {
      title: 'E2E Order of Service',
      date_time: new Date(Date.now() + 86400000).toISOString(),
    });
    expect(created.status).toBe(201);
    eventId = created.body.id;

    const empty = await apiClient.get<{ items: OrderItem[] }>(`/api/events/${eventId}/order`);
    expect(empty.status).toBe(200);
    expect(empty.body.items).toEqual([]);

    const saved = await apiClient.put<{ items: OrderItem[] }>(`/api/events/${eventId}/order`, {
      items: [
        { kind: 'block', title: 'Fohász' },
        { kind: 'song', title: '42. zsoltár', deckPath: '/tmp/42.pptx', notes: '1-3. vers' },
        { kind: 'bible', title: 'Lekció', bibleType: 'leckio' },
      ],
    });
    expect(saved.status).toBe(200);
    expect(saved.body.items.map((item) => item.kind)).toEqual(['block', 'song', 'bible']);
    expect(saved.body.items[1]?.notes).toBe('1-3. vers');

    const [first, second] = saved.body.items;
    const reordered = await apiClient.put<{ items: OrderItem[] }>(`/api/events/${eventId}/order`, {
      items: [second, first],
    });
    expect(reordered.body.items.map((item) => item.id)).toEqual([second?.id, first?.id]);
  });

  it('takes a song with no deck, but not one with neither title nor deck', async () => {
    const closing = await apiClient.put(`/api/events/${eventId}/order`, {
      items: [{ kind: 'song', title: 'Záró énekvers' }],
    });
    expect(closing.status).toBe(200);
    const nameless = await apiClient.put(`/api/events/${eventId}/order`, {
      items: [{ kind: 'song', title: '' }],
    });
    expect(nameless.status).toBe(400);
  });

  it('answers 404 for an unknown event', async () => {
    const res = await apiClient.get('/api/events/00000000-0000-0000-0000-000000000000/order');
    expect(res.status).toBe(404);
  });
});
