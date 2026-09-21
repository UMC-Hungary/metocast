/**
 * E2E tests for PPT WebSocket commands.
 */

import { describe, it, expect, beforeAll, afterAll } from 'vitest';
import { WsTestClient } from '../helpers/ws-client.js';

const isLive = !!process.env.TAURI_TEST_TOKEN;

describe.skipIf(!isLive)('PPT WebSocket Commands', () => {
  let ws: WsTestClient;

  beforeAll(async () => {
    ws = new WsTestClient();
    await ws.waitForConnect();
  });

  afterAll(() => {
    ws.close();
  });

  it('ppt.folders.list → ppt.folders.list response', async () => {
    const msg = await ws.command({ type: 'ppt.folders.list' }, 'ppt.folders.list');
    expect(msg['type']).toBe('ppt.folders.list');
    expect(Array.isArray(msg['folders'])).toBe(true);
  });

  it('ppt.folders.add → ppt.folders.add response', async () => {
    const msg = await ws.command(
      { type: 'ppt.folders.add', path: '/tmp/e2e-ws-ppt-test', name: 'WS E2E PPT Test' },
      'ppt.folders.add',
    );
    expect(msg['type']).toBe('ppt.folders.add');
  });
});
