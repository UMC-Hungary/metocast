/**
 * E2E tests for the Automations WebSocket commands and broadcasts.
 */

import { describe, it, expect, beforeAll, afterAll } from 'vitest';
import { apiClient } from '../helpers/client.js';
import { WsTestClient } from '../helpers/ws-client.js';

interface WsAutomation {
  id: string;
  name: string;
}

const isLive = !!process.env.TAURI_TEST_TOKEN;

const rule = (name: string) => ({
  name,
  triggers: [{ kind: 'middlecontrol.preset', preset: 1, cameraId: null }],
  actions: [{ kind: 'record.stop', target: { device: 'obs' } }],
});

describe.skipIf(!isLive)('Automations WebSocket', () => {
  let ws: WsTestClient;
  const created: string[] = [];

  beforeAll(async () => {
    ws = new WsTestClient();
    await ws.waitForConnect();
  });

  afterAll(async () => {
    ws.close();
    for (const id of created) await apiClient.delete(`/api/automations/${id}`);
    await apiClient.put('/api/automations/armed', { armed: true });
  });

  it('sends the rules to a client with the token as soon as it connects', async () => {
    const client = new WsTestClient();
    await client.waitForConnect();
    try {
      const msg = await client.waitForMessage('automations.changed');
      expect(typeof msg['armed']).toBe('boolean');
      expect(Array.isArray(msg['automations'])).toBe(true);
    } finally {
      client.close();
    }
  });

  it('automations.list → automations.changed', async () => {
    // The snapshot sent on connect is already waiting; this is about the answer.
    ws.discard('automations.changed');
    const msg = await ws.command({ type: 'automations.list' }, 'automations.changed');
    expect(msg['type']).toBe('automations.changed');
    expect(msg).toHaveProperty('armed');
    expect(Array.isArray(msg['automations'])).toBe(true);
  });

  it('every write is broadcast as automations.changed', async () => {
    // What arrived before this test is not about this write.
    ws.discard('automations.changed');

    const afterCreate = ws.waitForMessage('automations.changed');
    const res = await apiClient.post<WsAutomation>('/api/automations', rule('WS E2E rule'));
    expect(res.status).toBe(201);
    created.push(res.body.id);
    const created_ = await afterCreate;
    expect((created_['automations'] as WsAutomation[]).some((a) => a.id === res.body.id)).toBe(
      true,
    );

    const afterReplace = ws.waitForMessage('automations.changed');
    await apiClient.put(`/api/automations/${res.body.id}`, rule('WS E2E rule (renamed)'));
    const replaced = await afterReplace;
    expect(
      (replaced['automations'] as WsAutomation[]).find((a) => a.id === res.body.id)?.name,
    ).toBe('WS E2E rule (renamed)');

    const afterDelete = ws.waitForMessage('automations.changed');
    await apiClient.delete(`/api/automations/${res.body.id}`);
    created.splice(created.indexOf(res.body.id), 1);
    const deleted = await afterDelete;
    expect((deleted['automations'] as WsAutomation[]).some((a) => a.id === res.body.id)).toBe(
      false,
    );
  });

  it('automations.set_armed → ok, and the switch is broadcast', async () => {
    ws.discard('automations.changed');
    const changed = ws.waitForMessage('automations.changed');
    const ok = await ws.command({ type: 'automations.set_armed', armed: false }, 'ok');
    expect(ok['type']).toBe('ok');
    expect((await changed)['armed']).toBe(false);

    ws.discard('automations.changed');
    const back = ws.waitForMessage('automations.changed');
    await ws.command({ type: 'automations.set_armed', armed: true }, 'ok');
    expect((await back)['armed']).toBe(true);
  });

  it('automations.run of an unknown rule → error not_found', async () => {
    const msg = await ws.command(
      { type: 'automations.run', id: '00000000-0000-0000-0000-000000000000' },
      'error',
    );
    expect(msg['message']).toBe('not_found');
  });

  it('automations.run → ok, and the run arrives as automation.run', async () => {
    const res = await apiClient.post<WsAutomation>('/api/automations', rule('WS E2E run'));
    created.push(res.body.id);
    const run = ws.waitForMessage('automation.run');
    const ok = await ws.command({ type: 'automations.run', id: res.body.id }, 'ok');
    expect(ok['type']).toBe('ok');

    const broadcast = (await run)['run'] as {
      automationId: string;
      manual: boolean;
      results: Array<{ action: string; outcome: string; message: string | null }>;
    };
    expect(broadcast.automationId).toBe(res.body.id);
    expect(broadcast.manual).toBe(true);
    // Nothing is connected here, so OBS cannot be asked.
    expect(broadcast.results[0]).toMatchObject({
      action: 'Stop OBS',
      outcome: 'failed',
      message: 'obs_not_connected',
    });
  });

  it.each(['rodecaster.record.start', 'rodecaster.record.stop'])(
    '%s is refused while the RØDECaster is not connected',
    async (type) => {
      const msg = await ws.command({ type }, 'error');
      expect(msg['message']).toBe('rodecaster_not_connected');
    },
  );
});
