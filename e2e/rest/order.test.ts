/**
 * E2E tests for an event's Order of Service (`/api/events/{id}/order`).
 * Requires the isolated test server (see vitest.config.ts) with TAURI_TEST_TOKEN set.
 */

import { describe, it, expect, afterAll } from 'vitest';
import { apiClient } from '../helpers/client.js';

interface OrderItem {
  id: string;
  kind: 'song' | 'bible' | 'block' | 'announcements' | 'text';
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

  it('starts with the template, stores steps in order and keeps ids across edits', async () => {
    const created = await apiClient.post<{ id: string }>('/api/events', {
      title: 'E2E Order of Service',
      date_time: new Date(Date.now() + 86400000).toISOString(),
    });
    expect(created.status).toBe(201);
    eventId = created.body.id;

    const template = await apiClient.get<{ items: OrderItem[] }>(`/api/events/${eventId}/order`);
    expect(template.status).toBe(200);
    expect(template.body.items.map((item) => item.title)).toEqual([
      'Welcome message',
      'Song 1',
      'Song 2',
      'Lekció',
      'Song 3',
      'Textus',
      'Preaching',
      'Song 4',
      'Hirdetések',
      'Áldás',
      'Song 4 last part',
    ]);
    expect(
      template.body.items.filter((item) => item.kind === 'song').map((item) => item.deckPath),
    ).toEqual([null, null, null, null, null]);

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

  it('adds a reading step when a reading is set, and not again when it is re-sent', async () => {
    await apiClient.put(`/api/events/${eventId}/order`, {
      items: [{ kind: 'block', title: 'Köszöntés' }],
    });
    const update = (reference: string) =>
      apiClient.put(`/api/events/${eventId}`, {
        title: 'E2E Order of Service',
        date_time: new Date(Date.now() + 86400000).toISOString(),
        bible_references: [{ type: 'textus', reference, translation: 'RUF_v2' }],
      });
    expect((await update('Jn 3,16')).status).toBe(200);
    const added = await apiClient.get<{ items: OrderItem[] }>(`/api/events/${eventId}/order`);
    expect(added.body.items.map((item) => item.bibleType ?? item.kind)).toEqual([
      'block',
      'textus',
    ]);

    // The operator deletes the step; re-saving the same reading leaves it deleted.
    await apiClient.put(`/api/events/${eventId}/order`, { items: [added.body.items[0]] });
    await update('Jn 3,16');
    const kept = await apiClient.get<{ items: OrderItem[] }>(`/api/events/${eventId}/order`);
    expect(kept.body.items).toHaveLength(1);

    // A changed reading brings it back.
    await update('Jn 3,17');
    const back = await apiClient.get<{ items: OrderItem[] }>(`/api/events/${eventId}/order`);
    expect(back.body.items.map((item) => item.bibleType ?? item.kind)).toEqual(['block', 'textus']);
  });

  it('stores announcements with their bullets, but not without a title', async () => {
    const saved = await apiClient.put<{ items: OrderItem[] }>(`/api/events/${eventId}/order`, {
      items: [{ kind: 'announcements', title: 'Hirdetések', notes: 'Bibliaóra\nIfjúsági alkalom' }],
    });
    expect(saved.status).toBe(200);
    expect(saved.body.items[0]?.kind).toBe('announcements');
    expect(saved.body.items[0]?.notes).toBe('Bibliaóra\nIfjúsági alkalom');
    const untitled = await apiClient.put(`/api/events/${eventId}/order`, {
      items: [{ kind: 'announcements', title: '' }],
    });
    expect(untitled.status).toBe(400);
  });

  it('starts with a welcome slide and stores a text slide with paragraphs', async () => {
    const created = await apiClient.post<{ id: string }>('/api/events', {
      title: 'E2E welcome slide',
      date_time: new Date(Date.now() + 86400000).toISOString(),
    });
    const order = await apiClient.get<{ items: OrderItem[] }>(
      `/api/events/${created.body.id}/order`,
    );
    expect(order.body.items[0]).toMatchObject({ kind: 'text', title: 'Welcome message' });
    const saved = await apiClient.put<{ items: OrderItem[] }>(
      `/api/events/${created.body.id}/order`,
      {
        items: [{ kind: 'text', title: 'Köszöntjük!', notes: 'Isten hozott mindenkit' }],
      },
    );
    expect(saved.status).toBe(200);
    expect(saved.body.items[0]?.notes).toBe('Isten hozott mindenkit');
    await apiClient.delete(`/api/events/${created.body.id}`);
  });

  it('answers 404 for an unknown event', async () => {
    const res = await apiClient.get('/api/events/00000000-0000-0000-0000-000000000000/order');
    expect(res.status).toBe(404);
  });
});
