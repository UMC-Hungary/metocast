/**
 * E2E tests for Connectors WebSocket commands.
 */

import { describe, it, expect, beforeAll, afterAll } from 'vitest';
import { WsTestClient } from '../helpers/ws-client.js';

const isLive = !!process.env.TAURI_TEST_TOKEN;

describe.skipIf(!isLive)('Connectors WebSocket Commands', () => {
  let ws: WsTestClient;

  beforeAll(async () => {
    ws = new WsTestClient();
    await ws.waitForConnect();
  });

  afterAll(() => {
    ws.close();
  });

  it('connectors.status → connectors.status response', async () => {
    const msg = await ws.command({ type: 'connectors.status' }, 'connectors.status');
    expect(msg['type']).toBe('connectors.status');
    expect(msg).toHaveProperty('obs');
    expect(msg).toHaveProperty('vmix');
    expect(msg).toHaveProperty('youtube');
    expect(msg).toHaveProperty('facebook');
    expect(msg).toHaveProperty('rodecaster');
    expect(msg).toHaveProperty('middlecontrol');
    expect(msg).toHaveProperty('atem');
  });

  it('connectors.state → connectors.state response', async () => {
    const msg = await ws.command({ type: 'connectors.state' }, 'connectors.state');
    expect(msg['type']).toBe('connectors.state');
    expect(msg).toHaveProperty('obs');
    expect(msg).toHaveProperty('middlecontrol');
    expect(msg).toHaveProperty('atem');
  });

  it('broadcasts the initial middlecontrol connector status', async () => {
    const client = new WsTestClient();
    await client.waitForConnect();
    let message: Record<string, unknown> | undefined;
    for (let index = 0; index < 5; index += 1) {
      const candidate = await client.waitForMessage('connector.status');
      if (candidate['connector'] === 'middlecontrol') {
        message = candidate;
        break;
      }
    }
    client.close();
    expect(message?.['status']).toMatchObject({ type: expect.any(String) });
  });

  it.each([
    { type: 'middlecontrol.camera.select', camera_id: 1 },
    { type: 'middlecontrol.record.start' },
    { type: 'middlecontrol.record.stop', camera_id: 1 },
    { type: 'middlecontrol.record.start_all' },
    { type: 'middlecontrol.record.stop_all' },
    { type: 'middlecontrol.preset.recall', preset: 1 },
  ])('$type is refused while Middle Control is not connected', async (command) => {
    const msg = await ws.command(command, 'error');
    expect(msg['message']).toBe('middlecontrol_not_connected');
  });

  it.each([
    { type: 'atem.program.set', input: 1 },
    { type: 'atem.preview.set', input: 2 },
    { type: 'atem.cut' },
    { type: 'atem.auto' },
    { type: 'atem.record.start' },
    { type: 'atem.record.stop' },
    { type: 'atem.stream.start' },
    { type: 'atem.stream.stop' },
    { type: 'atem.stream.push_youtube' },
  ])('$type is refused while the ATEM is not connected', async (command) => {
    const msg = await ws.command(command, 'error');
    expect(msg['message']).toBe('atem_not_connected');
  });

  it.each([
    { type: 'obs.stream.start' },
    { type: 'obs.stream.stop' },
    { type: 'obs.record.start' },
    { type: 'obs.record.stop' },
  ])('$type is refused while OBS is not connected', async (command) => {
    const msg = await ws.command(command, 'error');
    expect(msg['message']).toBe('obs_not_connected');
  });

  // No RØDECaster is attached to CI, so these assert the refusal path. Muting a
  // real channel is checked by `rcast` against the desk.
  it('rodecaster.mute.set is refused while the desk is not connected', async () => {
    const msg = await ws.command({ type: 'rodecaster.mute.set', channel: 0, mute: true }, 'error');
    expect(msg['message']).toBe('rodecaster_not_connected');
  });

  it('rodecaster.profile is refused while the desk is not connected', async () => {
    const msg = await ws.command({ type: 'rodecaster.profile' }, 'error');
    expect(msg['message']).toBe('rodecaster_not_connected');
  });

  it('rodecaster audio discovery is refused while the desk is not connected', async () => {
    const msg = await ws.command({ type: 'rodecaster.audio.discover' }, 'error');
    expect(msg['message']).toBe('rodecaster_not_connected');
  });

  it('returns the authoritative idle RØDECaster recorder state', async () => {
    const msg = await ws.command(
      { type: 'rodecaster.audio.record.state' },
      'rodecaster.audio.record.state',
    );
    expect(msg['state']).toMatchObject({ status: 'idle', sessionId: null });
  });
});
