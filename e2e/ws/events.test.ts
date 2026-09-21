/**
 * E2E tests for Events WebSocket commands.
 * Requires the server running at localhost:3737 with TAURI_TEST_TOKEN set.
 */

import { describe, it, expect, beforeAll, afterAll } from 'vitest';
import { WsTestClient } from '../helpers/ws-client.js';

interface WsEvent {
  id: string;
  title: string;
}

const isLive = !!process.env.TAURI_TEST_TOKEN;

describe.skipIf(!isLive)('Events WebSocket Commands', () => {
  let ws: WsTestClient;
  let createdEventId: string;

  beforeAll(async () => {
    ws = new WsTestClient();
    await ws.waitForConnect();
  });

  afterAll(() => {
    ws.close();
  });

  it('events.list → events.list response', async () => {
    const msg = await ws.command({ type: 'events.list' }, 'events.list');
    expect(msg['type']).toBe('events.list');
    expect(Array.isArray(msg['events'])).toBe(true);
  });

  it('events.create → events.create response', async () => {
    const msg = await ws.command(
      {
        type: 'events.create',
        title: 'WS Test Event',
        date_time: new Date(Date.now() + 86400000).toISOString(),
        speaker: 'WS Speaker',
        description: 'Created via WS',
      },
      'events.create',
    );
    expect(msg['type']).toBe('events.create');
    const event = msg['event'] as WsEvent;
    expect(event).toHaveProperty('id');
    createdEventId = event.id;
  });

  it('events.get → events.get response', async () => {
    expect(createdEventId).toBeDefined();
    const msg = await ws.command({ type: 'events.get', id: createdEventId }, 'events.get');
    expect(msg['type']).toBe('events.get');
    const event = msg['event'] as WsEvent;
    expect(event.id).toBe(createdEventId);
  });

  it('events.update → events.update response', async () => {
    expect(createdEventId).toBeDefined();
    const msg = await ws.command(
      {
        type: 'events.update',
        id: createdEventId,
        title: 'WS Test Event (updated)',
        date_time: new Date(Date.now() + 86400000).toISOString(),
        speaker: 'WS Speaker',
        description: 'Updated via WS',
      },
      'events.update',
    );
    expect(msg['type']).toBe('events.update');
    const event = msg['event'] as WsEvent;
    expect(event.id).toBe(createdEventId);
  });

  it('events.delete → ok response', async () => {
    expect(createdEventId).toBeDefined();
    const msg = await ws.command({ type: 'events.delete', id: createdEventId }, 'ok');
    expect(msg['type']).toBe('ok');
    createdEventId = '';
  });

  it('events.get with unknown id → error response', async () => {
    const msg = await ws.command(
      { type: 'events.get', id: '00000000-0000-0000-0000-000000000000' },
      'error',
    );
    expect(msg['type']).toBe('error');
  });
});
