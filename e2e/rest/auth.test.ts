/**
 * E2E tests for the HTTP access contract.
 *
 * Locks down which endpoints are reachable without a bearer token. A route that
 * silently loses its auth layer is a security regression, so the split is
 * asserted rather than assumed.
 */

import { describe, it, expect } from 'vitest';
import WebSocket from 'ws';

const isLive = !!process.env.TAURI_TEST_TOKEN;
const BASE_URL = process.env.TAURI_TEST_BASE_URL ?? 'http://localhost:3737';
const WS_URL = process.env.TAURI_TEST_WS_URL ?? 'ws://localhost:3737';
const ADMIN_TOKEN = process.env.METOCAST_ADMIN_TOKEN ?? 'e2e-admin-token';

/** Endpoints that must answer without any credentials. */
const PUBLIC = [
  { method: 'GET', path: '/health' },
  { method: 'GET', path: '/openapi.json' },
  { method: 'GET', path: '/docs' },
  { method: 'GET', path: '/caption' },
];

/** Endpoints that must reject an unauthenticated caller. */
const AUTHENTICATED = [
  { method: 'GET', path: '/api/events' },
  { method: 'GET', path: '/api/connectors/status' },
  { method: 'GET', path: '/api/connectors/szentiras/config' },
  { method: 'PUT', path: '/api/connectors/szentiras/config' },
  { method: 'GET', path: '/api/bible/verses?reference=Jn%203,16&translation=RUF' },
  { method: 'GET', path: '/api/bible/suggest?term=Jn' },
  { method: 'GET', path: '/api/auth/youtube/url' },
  { method: 'POST', path: '/api/auth/youtube/logout' },
  { method: 'GET', path: '/api/auth/facebook/url' },
  { method: 'POST', path: '/api/auth/facebook/logout' },
  { method: 'POST', path: '/api/connectors/blackmagic-camera/discover' },
  { method: 'POST', path: '/api/connectors/middlecontrol/discover' },
  { method: 'POST', path: '/api/connectors/blackmagic-camera/stream/youtube' },
  { method: 'POST', path: '/api/connectors/atem/stream/youtube' },
  { method: 'POST', path: '/api/connectors/atem/discover' },
  { method: 'GET', path: '/api/queues' },
  { method: 'GET', path: '/api/settings/song-slide-folder' },
  { method: 'PUT', path: '/api/settings/song-slide-folder' },
  { method: 'POST', path: '/api/ppt/song' },
  { method: 'GET', path: '/api/connectors/szentiras/config/secrets' },
];

describe.skipIf(!isLive)('HTTP access contract', () => {
  it.each(PUBLIC)('$method $path is reachable without a token', async ({ method, path }) => {
    const res = await fetch(`${BASE_URL}${path}`, { method });
    expect(res.status).toBeLessThan(400);
  });

  it.each(AUTHENTICATED)('$method $path rejects a missing token', async ({ method, path }) => {
    const res = await fetch(`${BASE_URL}${path}`, { method });
    expect(res.status).toBe(401);
  });

  it('refuses a secret read without the admin token', async () => {
    const res = await fetch(`${BASE_URL}/api/connectors/szentiras/config/secrets`, {
      headers: { Authorization: `Bearer ${process.env.TAURI_TEST_TOKEN}` },
    });
    expect(res.status).toBe(403);
    expect(await res.text()).not.toContain('secret');
  });

  it('refuses a secret read with a wrong admin token', async () => {
    const res = await fetch(`${BASE_URL}/api/connectors/szentiras/config/secrets`, {
      headers: {
        Authorization: `Bearer ${process.env.TAURI_TEST_TOKEN}`,
        'X-Admin-Token': 'not-the-admin-token',
      },
    });
    expect(res.status).toBe(403);
  });

  it('refuses a secret read that is unauthenticated, even with the admin token', async () => {
    const res = await fetch(`${BASE_URL}/api/connectors/szentiras/config/secrets`, {
      headers: { 'X-Admin-Token': ADMIN_TOKEN },
    });
    expect(res.status).toBe(401);
  });

  it('returns the secret to a loopback caller holding both tokens', async () => {
    const stored = 'e2e-reveal-me';
    await fetch(`${BASE_URL}/api/connectors/szentiras/config`, {
      method: 'PUT',
      headers: {
        Authorization: `Bearer ${process.env.TAURI_TEST_TOKEN}`,
        'Content-Type': 'application/json',
      },
      body: JSON.stringify({ enabled: true, apiKey: stored }),
    });

    const res = await fetch(`${BASE_URL}/api/connectors/szentiras/config/secrets`, {
      headers: {
        Authorization: `Bearer ${process.env.TAURI_TEST_TOKEN}`,
        'X-Admin-Token': ADMIN_TOKEN,
      },
    });
    expect(res.status).toBe(200);
    expect(((await res.json()) as { apiKey: string }).apiKey).toBe(stored);

    await fetch(`${BASE_URL}/api/connectors/szentiras/config`, {
      method: 'PUT',
      headers: {
        Authorization: `Bearer ${process.env.TAURI_TEST_TOKEN}`,
        'Content-Type': 'application/json',
      },
      body: JSON.stringify({ enabled: false, apiKey: '', apiKeySet: false }),
    });
  });

  it('rejects a wrong token', async () => {
    const res = await fetch(`${BASE_URL}/api/connectors/status`, {
      headers: { Authorization: 'Bearer not-the-token' },
    });
    expect(res.status).toBe(401);
  });

  it('rejects a token passed without the Bearer scheme', async () => {
    const res = await fetch(`${BASE_URL}/api/connectors/status`, {
      headers: { Authorization: process.env.TAURI_TEST_TOKEN ?? '' },
    });
    expect(res.status).toBe(401);
  });

  it('answers a plain HTTP request to /ws with 426, not a token leak', async () => {
    const res = await fetch(`${BASE_URL}/ws`);
    expect(res.status).toBe(426);
    expect(await res.text()).not.toContain(process.env.TAURI_TEST_TOKEN ?? 'unset');
  });

  it('lets an unauthenticated WebSocket connect but confines it to read-only commands', async () => {
    const ws = new WebSocket(`${WS_URL}/ws`);
    const messages: Record<string, unknown>[] = [];
    await new Promise<void>((resolve, reject) => {
      ws.on('open', () => resolve());
      ws.on('error', reject);
    });
    ws.on('message', (data: WebSocket.Data) => {
      messages.push(JSON.parse(data.toString()) as Record<string, unknown>);
    });

    // A privileged command must be refused...
    ws.send(JSON.stringify({ type: 'events.list' }));
    await new Promise((resolve) => setTimeout(resolve, 500));
    expect(messages.some((m) => m['type'] === 'error' && m['message'] === 'unauthorized')).toBe(
      true,
    );
    expect(messages.some((m) => m['type'] === 'events.list')).toBe(false);

    // ...while the presenter read-only allowlist still works.
    messages.length = 0;
    ws.send(JSON.stringify({ type: 'presenter.status' }));
    await new Promise((resolve) => setTimeout(resolve, 500));
    expect(messages.some((m) => m['type'] === 'error')).toBe(false);

    ws.close();
  });
});
