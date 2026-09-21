/**
 * E2E tests for Cron Jobs WebSocket commands.
 */

import { describe, it, expect, beforeAll, afterAll } from 'vitest';
import { WsTestClient } from '../helpers/ws-client.js';

interface WsCronJob {
  id: string;
  name: string;
}

const isLive = !!process.env.TAURI_TEST_TOKEN;

describe.skipIf(!isLive)('Cron Jobs WebSocket Commands', () => {
  let ws: WsTestClient;
  let cronJobId: string;

  beforeAll(async () => {
    ws = new WsTestClient();
    await ws.waitForConnect();
  });

  afterAll(() => {
    ws.close();
  });

  it('cron_jobs.list → cron_jobs.list response', async () => {
    const msg = await ws.command({ type: 'cron_jobs.list' }, 'cron_jobs.list');
    expect(msg['type']).toBe('cron_jobs.list');
    expect(Array.isArray(msg['jobs'])).toBe(true);
  });

  it('cron_jobs.create → cron_jobs.create response', async () => {
    const msg = await ws.command(
      {
        type: 'cron_jobs.create',
        name: 'WS E2E Test Job',
        cron_expression: '0 0 9 * * 1',
        enabled: false,
        pull_youtube: false,
        auto_upload: false,
      },
      'cron_jobs.create',
    );
    expect(msg['type']).toBe('cron_jobs.create');
    const job = msg['job'] as WsCronJob;
    expect(job).toHaveProperty('id');
    cronJobId = job.id;
  });

  it('cron_jobs.update → cron_jobs.update response', async () => {
    expect(cronJobId).toBeDefined();
    const msg = await ws.command(
      {
        type: 'cron_jobs.update',
        id: cronJobId,
        name: 'WS E2E Test Job (updated)',
        cron_expression: '0 0 10 * * 1',
        enabled: true,
        pull_youtube: false,
        auto_upload: false,
      },
      'cron_jobs.update',
    );
    expect(msg['type']).toBe('cron_jobs.update');
  });

  it('cron_jobs.delete → ok response', async () => {
    expect(cronJobId).toBeDefined();
    const msg = await ws.command({ type: 'cron_jobs.delete', id: cronJobId }, 'ok');
    expect(msg['type']).toBe('ok');
  });
});
