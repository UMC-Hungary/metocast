/**
 * E2E tests for the Automations REST API.
 *
 * CI has no hardware, which makes the failure path deterministic: a rule that is run with nothing
 * connected must come back with every action failed, each with its connector's not-connected code.
 */

import { describe, it, expect, afterAll } from 'vitest';
import { apiClient } from '../helpers/client.js';
import { WsTestClient } from '../helpers/ws-client.js';

interface Automation {
  id: string;
  name: string;
  enabled: boolean;
  triggers: Array<Record<string, unknown>>;
  actions: Array<Record<string, unknown>>;
  cooldownSeconds: number;
  requireEvent: boolean;
  createdAt: string;
  updatedAt: string;
}

interface AutomationList {
  armed: boolean;
  automations: Automation[];
}

interface ActionResult {
  action: string;
  outcome: 'done' | 'skipped' | 'failed';
  message: string | null;
}

interface AutomationRun {
  id: string;
  automationId: string;
  name: string;
  cause: string;
  manual: boolean;
  startedAt: string;
  results: ActionResult[];
}

const isLive = !!process.env.TAURI_TEST_TOKEN;

/** The plan's "Service start", with every recorder. */
function serviceStart(overrides: Record<string, unknown> = {}): Record<string, unknown> {
  return {
    name: 'E2E Service start',
    enabled: true,
    triggers: [
      { kind: 'middlecontrol.preset', preset: 3, cameraId: null },
      {
        kind: 'rodecaster.channel',
        channel: 2,
        label: 'Wireless 1',
        live: true,
        source: 'either',
        holdSeconds: 0,
      },
    ],
    actions: [
      { kind: 'middlecontrol.preset.recall', preset: 3, cameraId: null, waitForArrival: true },
      { kind: 'record.start', target: { device: 'obs' } },
      { kind: 'record.start', target: { device: 'atem' } },
      { kind: 'record.start', target: { device: 'blackmagic-camera' } },
      { kind: 'record.start', target: { device: 'middlecontrol', cameraId: null } },
      { kind: 'record.start', target: { device: 'rodecaster' } },
      { kind: 'record.start', target: { device: 'rodecaster-audio' } },
    ],
    cooldownSeconds: 5,
    requireEvent: false,
    ...overrides,
  };
}

describe.skipIf(!isLive)('Automations REST API', () => {
  const created: string[] = [];

  async function create(overrides: Record<string, unknown> = {}): Promise<Automation> {
    const res = await apiClient.post<Automation>('/api/automations', serviceStart(overrides));
    expect(res.status).toBe(201);
    created.push(res.body.id);
    return res.body;
  }

  afterAll(async () => {
    for (const id of created) await apiClient.delete(`/api/automations/${id}`);
    // A test that disarmed the core must not leave it disarmed.
    await apiClient.put('/api/automations/armed', { armed: true });
  });

  it('GET /api/automations → 200 with the master switch and the rules', async () => {
    const res = await apiClient.get<AutomationList>('/api/automations');
    expect(res.status).toBe(200);
    expect(typeof res.body.armed).toBe('boolean');
    expect(Array.isArray(res.body.automations)).toBe(true);
  });

  it('POST /api/automations → 201 with the rule as stored', async () => {
    const rule = await create();
    expect(rule).toMatchObject({
      name: 'E2E Service start',
      enabled: true,
      cooldownSeconds: 5,
      requireEvent: false,
    });
    expect(rule.id).toMatch(/^[0-9a-f-]{36}$/);
    expect(rule.triggers).toHaveLength(2);
    expect(rule.actions).toHaveLength(7);
    // The wire format is the documented one, field for field.
    expect(rule.triggers[0]).toEqual({ kind: 'middlecontrol.preset', preset: 3, cameraId: null });
    expect(rule.triggers[1]).toEqual({
      kind: 'rodecaster.channel',
      channel: 2,
      label: 'Wireless 1',
      live: true,
      source: 'either',
      holdSeconds: 0,
    });
    expect(rule.actions[0]).toEqual({
      kind: 'middlecontrol.preset.recall',
      preset: 3,
      cameraId: null,
      waitForArrival: true,
    });
    expect(rule.actions[4]).toEqual({
      kind: 'record.start',
      target: { device: 'middlecontrol', cameraId: null },
    });
  });

  it('the rule shows in the list', async () => {
    const rule = await create({ name: 'E2E Listed' });
    const res = await apiClient.get<AutomationList>('/api/automations');
    expect(res.body.automations.find((a) => a.id === rule.id)?.name).toBe('E2E Listed');
  });

  it('fills the documented defaults for what a client leaves out', async () => {
    const res = await apiClient.post<Automation>('/api/automations', {
      name: 'E2E Defaults',
      triggers: [{ kind: 'rodecaster.channel', channel: 0, live: false }],
      actions: [{ kind: 'record.stop', target: { device: 'obs' } }],
    });
    expect(res.status).toBe(201);
    created.push(res.body.id);
    expect(res.body.enabled).toBe(true);
    expect(res.body.cooldownSeconds).toBe(5);
    expect(res.body.requireEvent).toBe(false);
    expect(res.body.triggers[0]).toMatchObject({ source: 'either', holdSeconds: 0, label: '' });
  });

  describe('validation', () => {
    it.each([
      [
        'preset 13 in a trigger',
        { triggers: [{ kind: 'middlecontrol.preset', preset: 13, cameraId: null }] },
        'preset',
      ],
      [
        'preset 0 in a trigger',
        { triggers: [{ kind: 'middlecontrol.preset', preset: 0, cameraId: null }] },
        'preset',
      ],
      [
        'camera 100 in a trigger',
        { triggers: [{ kind: 'middlecontrol.preset', preset: 3, cameraId: 100 }] },
        'camera_id',
      ],
      [
        'preset 13 in a camera move',
        {
          actions: [
            {
              kind: 'middlecontrol.preset.recall',
              preset: 13,
              cameraId: null,
              waitForArrival: true,
            },
          ],
        },
        'preset',
      ],
      [
        'camera 0 on a Middle Control recorder',
        { actions: [{ kind: 'record.start', target: { device: 'middlecontrol', cameraId: 0 } }] },
        'camera_id',
      ],
      [
        'a 61 s hold',
        {
          triggers: [
            {
              kind: 'rodecaster.channel',
              channel: 2,
              label: '',
              live: false,
              source: 'either',
              holdSeconds: 61,
            },
          ],
        },
        'holdSeconds',
      ],
      ['a 0.5 s cooldown', { cooldownSeconds: 0.5 }, 'cooldownSeconds'],
      ['a 601 s cooldown', { cooldownSeconds: 601 }, 'cooldownSeconds'],
      ['no trigger', { triggers: [] }, 'trigger'],
      ['no action', { actions: [] }, 'action'],
      ['a blank name', { name: '   ' }, 'name'],
      ['an 81-character name', { name: 'x'.repeat(81) }, 'name'],
    ])('POST refuses %s with 400 and says why', async (_what, overrides, word) => {
      const res = await apiClient.post<{ error: string }>(
        '/api/automations',
        serviceStart(overrides),
      );
      expect(res.status).toBe(400);
      expect(res.body.error).toContain(word);
    });

    it('PUT refuses a rule that breaks a limit too', async () => {
      const rule = await create();
      const res = await apiClient.put<{ error: string }>(
        `/api/automations/${rule.id}`,
        serviceStart({ cooldownSeconds: 0 }),
      );
      expect(res.status).toBe(400);
      // And the stored rule is untouched.
      const list = await apiClient.get<AutomationList>('/api/automations');
      expect(list.body.automations.find((a) => a.id === rule.id)?.cooldownSeconds).toBe(5);
    });

    it('accepts the limits themselves', async () => {
      const rule = await create({
        name: 'x'.repeat(80),
        cooldownSeconds: 600,
        triggers: [{ kind: 'middlecontrol.preset', preset: 12, cameraId: 99 }],
      });
      expect(rule.cooldownSeconds).toBe(600);
    });
  });

  it('PUT /api/automations/:id → 200 replaces the whole rule', async () => {
    const rule = await create();
    const res = await apiClient.put<Automation>(
      `/api/automations/${rule.id}`,
      serviceStart({
        name: 'E2E Replaced',
        enabled: false,
        actions: [{ kind: 'record.stop', target: { device: 'obs' } }],
      }),
    );
    expect(res.status).toBe(200);
    expect(res.body).toMatchObject({ id: rule.id, name: 'E2E Replaced', enabled: false });
    expect(res.body.actions).toHaveLength(1);
  });

  it('PUT and DELETE of an unknown rule → 404', async () => {
    const unknown = '00000000-0000-0000-0000-000000000000';
    expect((await apiClient.put(`/api/automations/${unknown}`, serviceStart())).status).toBe(404);
    expect((await apiClient.delete(`/api/automations/${unknown}`)).status).toBe(404);
    expect((await apiClient.post(`/api/automations/${unknown}/run`)).status).toBe(404);
  });

  it('DELETE /api/automations/:id → 204, and the rule is gone', async () => {
    const rule = await create();
    expect((await apiClient.delete(`/api/automations/${rule.id}`)).status).toBe(204);
    const list = await apiClient.get<AutomationList>('/api/automations');
    expect(list.body.automations.some((a) => a.id === rule.id)).toBe(false);
  });

  it('PUT /api/automations/armed → the master switch is saved', async () => {
    const off = await apiClient.put<{ armed: boolean }>('/api/automations/armed', { armed: false });
    expect(off.status).toBe(200);
    expect(off.body.armed).toBe(false);
    expect((await apiClient.get<AutomationList>('/api/automations')).body.armed).toBe(false);

    await apiClient.put('/api/automations/armed', { armed: true });
    expect((await apiClient.get<AutomationList>('/api/automations')).body.armed).toBe(true);
  });

  describe('Run now, with nothing connected', () => {
    it('returns a run in which every action failed with its connector’s not-connected code', async () => {
      const rule = await create();
      const res = await apiClient.post<AutomationRun>(`/api/automations/${rule.id}/run`);
      expect(res.status).toBe(200);

      expect(res.body).toMatchObject({
        automationId: rule.id,
        name: 'E2E Service start',
        cause: 'Run now',
        manual: true,
      });
      expect(res.body.results.map((r) => [r.action, r.outcome, r.message])).toEqual([
        ['Move camera to preset 3', 'failed', 'middlecontrol_not_connected'],
        ['Start OBS', 'failed', 'obs_not_connected'],
        ['Start ATEM', 'failed', 'atem_not_connected'],
        ['Start Blackmagic camera', 'failed', 'blackmagic_camera_not_connected'],
        ['Start all Middle Control cameras', 'failed', 'middlecontrol_not_connected'],
        ['Start RØDECaster SD', 'failed', 'rodecaster_not_connected'],
        ['Start Metocast audio', 'failed', 'rodecaster_not_connected'],
      ]);
    });

    it('runs although the master switch is off, the rule is disabled and its cooldown is long', async () => {
      const rule = await create({ enabled: false, cooldownSeconds: 600, requireEvent: true });
      await apiClient.put('/api/automations/armed', { armed: false });
      try {
        const first = await apiClient.post<AutomationRun>(`/api/automations/${rule.id}/run`);
        const second = await apiClient.post<AutomationRun>(`/api/automations/${rule.id}/run`);
        expect(first.status).toBe(200);
        expect(second.status).toBe(200);
        expect(second.body.manual).toBe(true);
      } finally {
        await apiClient.put('/api/automations/armed', { armed: true });
      }
    });

    it('keeps the run in the history, newest first', async () => {
      const rule = await create({ name: 'E2E History' });
      const run = await apiClient.post<AutomationRun>(`/api/automations/${rule.id}/run`);
      const history = await apiClient.get<AutomationRun[]>('/api/automations/runs');
      expect(history.status).toBe(200);
      expect(history.body.length).toBeLessThanOrEqual(50);
      expect(history.body[0]?.id).toBe(run.body.id);
      expect(history.body[0]?.name).toBe('E2E History');
    });

    it('tells every client: an automation.run and a warning', async () => {
      const rule = await create({ name: 'E2E Broadcast' });
      const ws = new WsTestClient();
      await ws.waitForConnect();
      try {
        const run = ws.waitForMessage('automation.run');
        const warning = ws.waitForMessage('notification');
        const res = await apiClient.post<AutomationRun>(`/api/automations/${rule.id}/run`);

        const broadcast = await run;
        expect((broadcast['run'] as AutomationRun).id).toBe(res.body.id);
        const notification = await warning;
        expect(notification['level']).toBe('warn');
        expect(notification['message']).toContain('E2E Broadcast');
        expect(notification['message']).toContain('obs_not_connected');
      } finally {
        ws.close();
      }
    });

    it('a stop on a recorder that is not recording is skipped, not failed', async () => {
      // Metocast's own audio recording is idle on a server with no desk, and idle is what a stop
      // wants, so the action reads its state first and has nothing to do.
      const rule = await create({
        name: 'E2E Quiet',
        actions: [{ kind: 'record.stop', target: { device: 'rodecaster-audio' } }],
      });
      const res = await apiClient.post<AutomationRun>(`/api/automations/${rule.id}/run`);
      expect(res.body.results).toEqual([
        { action: 'Stop Metocast audio', outcome: 'skipped', message: 'not recording' },
      ]);
    });
  });
});
