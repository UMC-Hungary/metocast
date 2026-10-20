/**
 * E2E tests for the MCP server at POST /mcp.
 *
 * Speaks plain JSON-RPC over fetch, so no MCP SDK is needed. Each test turns MCP on or off
 * through the connector config, the same switch the Mac app uses.
 */

import { networkInterfaces } from 'node:os';
import { afterAll, beforeAll, describe, expect, it } from 'vitest';
import { apiClient } from '../helpers/client';

const isLive = !!process.env.TAURI_TEST_TOKEN;
const BASE_URL = process.env.TAURI_TEST_BASE_URL ?? 'http://localhost:3738';
const TOKEN = process.env.TAURI_TEST_TOKEN ?? '';

type Json = Record<string, unknown>;
interface Rpc {
  status: number;
  session: string | null;
  body: { result?: Json; error?: { code: number; message: string } } | null;
}

async function rpc(
  method: string,
  params: Json = {},
  headers: Record<string, string> = {},
  id: number | null = 1,
): Promise<Rpc> {
  const res = await fetch(`${BASE_URL}/mcp`, {
    method: 'POST',
    headers: {
      Authorization: `Bearer ${TOKEN}`,
      'Content-Type': 'application/json',
      Accept: 'application/json, text/event-stream',
      ...headers,
    },
    body: JSON.stringify({ jsonrpc: '2.0', ...(id === null ? {} : { id }), method, params }),
  });
  const text = await res.text();
  return {
    status: res.status,
    session: res.headers.get('mcp-session-id'),
    body: text ? (JSON.parse(text) as Rpc['body']) : null,
  };
}

async function call(name: string, args: Json = {}) {
  const res = await rpc('tools/call', { name, arguments: args });
  const result = res.body?.result as { content: { text: string }[]; isError: boolean };
  return { isError: result.isError, text: result.content[0]?.text ?? '' };
}

/** This machine's LAN address: a call to it arrives from that address, not from loopback. */
const lanAddress = Object.values(networkInterfaces())
  .flat()
  .find((entry) => entry?.family === 'IPv4' && !entry.internal)?.address;

const setConfig = (config: Json) => apiClient.put('/api/connectors/mcp/config', config);

describe.skipIf(!isLive)('MCP server', () => {
  beforeAll(async () => {
    await setConfig({ enabled: true, readOnly: false, allowLan: false });
  });
  afterAll(async () => {
    await setConfig({ enabled: false, readOnly: false, allowLan: false });
  });

  it('is off until the operator turns it on', async () => {
    await setConfig({ enabled: false });
    expect((await rpc('ping')).status).toBe(503);
    await setConfig({ enabled: true });
    expect((await rpc('ping')).status).toBe(200);
  });

  it('rejects a wrong token', async () => {
    const res = await fetch(`${BASE_URL}/mcp`, {
      method: 'POST',
      headers: { Authorization: 'Bearer nope', 'Content-Type': 'application/json' },
      body: '{}',
    });
    expect(res.status).toBe(401);
  });

  it('refuses a caller that is not on loopback until LAN access is allowed', async (ctx) => {
    const url = `http://${lanAddress}:${new URL(BASE_URL).port}/mcp`;
    // A host firewall can drop connections to the LAN address; nothing to test then.
    const reachable = await fetch(url.replace('/mcp', '/health')).then(
      (res) => res.ok,
      () => false,
    );
    if (!lanAddress || !reachable) return ctx.skip();
    const ping = () =>
      fetch(url, {
        method: 'POST',
        headers: { Authorization: `Bearer ${TOKEN}`, 'Content-Type': 'application/json' },
        body: JSON.stringify({ jsonrpc: '2.0', id: 1, method: 'ping' }),
      });
    await setConfig({ enabled: true, allowLan: false });
    expect((await ping()).status).toBe(403);
    await setConfig({ enabled: true, allowLan: true });
    expect((await ping()).status).toBe(200);
    await setConfig({ enabled: true, allowLan: false });
    expect((await ping()).status).toBe(403);
  });

  it('refuses a browser origin that is not local', async () => {
    const res = await rpc('ping', {}, { Origin: 'https://evil.example' });
    expect(res.status).toBe(403);
  });

  it('initializes, issues a session and lists tools with safety annotations', async () => {
    const init = await rpc('initialize', {
      protocolVersion: '2025-06-18',
      capabilities: {},
      clientInfo: { name: 'e2e', version: '1' },
    });
    expect(init.status).toBe(200);
    expect(init.session).toBeTruthy();
    expect(init.body?.result?.serverInfo).toMatchObject({ name: 'metocast' });

    const list = await rpc('tools/list', {}, { 'Mcp-Session-Id': init.session ?? '' });
    const tools = list.body?.result?.tools as {
      name: string;
      inputSchema: { type: string };
      annotations: { readOnlyHint: boolean; destructiveHint: boolean };
    }[];
    expect(tools.length).toBeGreaterThan(150);
    const byName = new Map(tools.map((t) => [t.name, t]));
    expect(byName.get('events_list')?.annotations.readOnlyHint).toBe(true);
    expect(byName.get('events_delete')?.annotations.destructiveHint).toBe(true);
    expect(byName.get('obs_stream_start')?.annotations.destructiveHint).toBe(true);
    expect(byName.get('events_get')?.inputSchema.type).toBe('object');
    // The secrets route has no tool.
    expect([...byName.keys()].some((n) => n.includes('secrets'))).toBe(false);
  });

  it('shows the session in clients.list', async () => {
    const init = await rpc('initialize', { clientInfo: { name: 'e2e-visible' } });
    const clients = await call('clients_list');
    expect(clients.text).toContain('MCP · e2e-visible');
    await fetch(`${BASE_URL}/mcp`, {
      method: 'DELETE',
      headers: { Authorization: `Bearer ${TOKEN}`, 'Mcp-Session-Id': init.session ?? '' },
    });
  });

  it('answers 404 for a session the server does not know', async () => {
    const res = await rpc('ping', {}, { 'Mcp-Session-Id': '00000000-0000-4000-8000-000000000000' });
    expect(res.status).toBe(404);
  });

  it('runs a REST-only tool through the real router', async () => {
    const res = await call('settings_title_template_get');
    expect(res.isError).toBe(false);
    expect(() => JSON.parse(res.text)).not.toThrow();
  });

  it('runs an event through create, get, list and delete', async () => {
    const created = await call('events_create', {
      title: 'MCP e2e event',
      date_time: new Date(Date.now() + 86_400_000).toISOString(),
    });
    expect(created.isError).toBe(false);
    const event = JSON.parse(created.text) as { event?: { id: string }; id?: string };
    const id = event.event?.id ?? event.id;
    expect(id).toBeTruthy();

    const got = await call('events_get', { id });
    expect(got.isError).toBe(false);
    expect(got.text).toContain('MCP e2e event');

    const listed = await call('events_list');
    expect(listed.text).toContain('MCP e2e event');

    // A new event starts with the default Order of Service, whichever way it was created.
    const order = await call('events_order_get', { id });
    expect(order.isError).toBe(false);
    expect((JSON.parse(order.text) as { items: unknown[] }).items).toHaveLength(10);

    const removed = await call('events_delete', { id });
    expect(removed.isError).toBe(false);
  });

  it('turns a failing command into a tool error, not a protocol error', async () => {
    const res = await call('events_get', { id: '00000000-0000-4000-8000-000000000000' });
    expect(res.isError).toBe(true);
    const bad = await call('events_get', { id: 'not-a-uuid' });
    expect(bad.isError).toBe(true);
    expect(bad.text).toContain('Invalid arguments');
  });

  it('reports an unknown tool as an invalid-params error', async () => {
    const res = await rpc('tools/call', { name: 'no_such_tool', arguments: {} });
    expect(res.body?.error?.code).toBe(-32602);
  });

  it('events_poll returns what REST caused, after a cursor', async () => {
    await rpc('initialize', { clientInfo: { name: 'e2e-poll' } });
    const first = JSON.parse((await call('events_poll', {})).text) as { next: number };
    const made = await apiClient.post<{ id: string }>('/api/events', {
      title: 'MCP poll event',
      date_time: new Date(Date.now() + 86_400_000).toISOString(),
    });
    expect(made.status).toBeLessThan(300);
    const polled = JSON.parse(
      (
        await call('events_poll', {
          since: first.next,
          types: ['event.changed'],
          wait_ms: 8000,
        })
      ).text,
    ) as { events: { event: { type: string } }[]; next: number };
    expect(polled.events.length).toBeGreaterThan(0);
    expect(polled.events[0]?.event.type).toBe('event.changed');
    expect(polled.next).toBeGreaterThan(first.next);
    await apiClient.delete(`/api/events/${made.body.id}`);
  });

  it('read-only mode hides and refuses every tool that changes something', async () => {
    await setConfig({ enabled: true, readOnly: true });
    const list = await rpc('tools/list');
    const names = (list.body?.result?.tools as { name: string }[]).map((t) => t.name);
    expect(names).toContain('events_list');
    expect(names).not.toContain('events_create');
    expect(names).not.toContain('events_delete');
    const refused = await call('events_delete', { id: '00000000-0000-4000-8000-000000000000' });
    expect(refused.isError).toBe(true);
    expect(refused.text).toContain('read-only');
    await setConfig({ enabled: true, readOnly: false });
  });

  it('does not let an agent change its own access', async () => {
    const res = await call('connectors_config_set', {
      name: 'mcp',
      body: { enabled: true, allowLan: true },
    });
    expect(res.isError).toBe(true);
    expect(res.text).toContain('operator');
    const stored = await apiClient.get<{ allowLan: boolean }>('/api/connectors/mcp/config');
    expect(stored.body.allowLan).toBe(false);
  });

  it('serves the manual as resources', async () => {
    const res = await rpc('resources/read', { uri: 'metocast://docs/mcp-tools.md' });
    const contents = res.body?.result?.contents as { text: string }[];
    expect(contents[0]?.text).toContain('MCP tool reference');
  });

  it('documents every route in the OpenAPI spec', async () => {
    const spec = (await (await fetch(`${BASE_URL}/openapi.json`)).json()) as {
      paths: Record<string, unknown>;
    };
    for (const path of ['/api/media/programs/{id}/nudge', '/api/queues', '/api/logs']) {
      expect(spec.paths[path]).toBeTruthy();
    }
  });
});
