/**
 * `metocast-server mcp-stdio`, the bridge for clients that can only start a program (Claude
 * Desktop). The built binary is started the way such a client starts it, through the SDK's stdio
 * transport, and talks to the same isolated server the other MCP tests use.
 */

import { join } from 'node:path';
import { afterAll, beforeAll, describe, expect, it } from 'vitest';
import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { StdioClientTransport } from '@modelcontextprotocol/sdk/client/stdio.js';
import { ResourceUpdatedNotificationSchema } from '@modelcontextprotocol/sdk/types.js';
import { apiClient } from '../helpers/client';

const isLive = !!process.env.TAURI_TEST_TOKEN;
const BASE_URL = process.env.TAURI_TEST_BASE_URL ?? 'http://localhost:3738';
const TOKEN = process.env.TAURI_TEST_TOKEN ?? '';
const BINARY = join(
  process.cwd(),
  'target',
  'debug',
  process.platform === 'win32' ? 'metocast-server.exe' : 'metocast-server',
);
const NAME = 'stdio-bridge-e2e';

const setConfig = (enabled: boolean) =>
  apiClient.put('/api/connectors/mcp/config', { enabled, readOnly: false, allowLan: false });

type Json = Record<string, unknown>;

/** One JSON-RPC call straight to `/mcp`, with no bridge in between: the reference to compare to. */
async function direct(method: string, params: Json = {}, session?: string) {
  const res = await fetch(`${BASE_URL}/mcp`, {
    method: 'POST',
    headers: {
      Authorization: `Bearer ${TOKEN}`,
      'Content-Type': 'application/json',
      Accept: 'application/json, text/event-stream',
      ...(session ? { 'Mcp-Session-Id': session } : {}),
    },
    body: JSON.stringify({ jsonrpc: '2.0', id: 1, method, params }),
  });
  const body = (await res.json()) as { result: Json };
  return { session: res.headers.get('mcp-session-id') ?? '', result: body.result };
}

describe.skipIf(!isLive)('MCP stdio bridge', () => {
  let reference = '';
  let bridge: Client;

  beforeAll(async () => {
    await setConfig(true);
    reference = (await direct('initialize', { clientInfo: { name: `${NAME}-direct` } })).session;
    bridge = new Client({ name: NAME, version: '1.0.0' });
    await bridge.connect(
      new StdioClientTransport({
        command: BINARY,
        args: ['mcp-stdio'],
        // The token is in the environment, never on the command line.
        env: { METOCAST_MCP_URL: `${BASE_URL}/mcp`, METOCAST_MCP_TOKEN: TOKEN },
      }),
    );
  });

  afterAll(async () => {
    await bridge.close();
    await fetch(`${BASE_URL}/mcp`, {
      method: 'DELETE',
      headers: { Authorization: `Bearer ${TOKEN}`, 'Mcp-Session-Id': reference },
    });
    await setConfig(false);
  });

  /** The ids of the agents Metocast lists as connected under this client name. */
  async function bridgeSessions(): Promise<string[]> {
    const { result } = await direct(
      'tools/call',
      { name: 'clients_list', arguments: {} },
      reference,
    );
    const [first] = result.content as { text: string }[];
    const { clients } = JSON.parse(first?.text ?? '{}') as {
      clients: { id: string; label: string }[];
    };
    return clients.filter((client) => client.label === `MCP · ${NAME}`).map((client) => client.id);
  }

  it('initializes and lists exactly the tools the HTTP endpoint lists', async () => {
    expect(bridge.getServerVersion()?.name).toBe('metocast');
    const viaBridge = (await bridge.listTools()).tools.map((tool) => tool.name).sort();
    const { result } = await direct('tools/list', {}, reference);
    const catalogue = (result.tools as { name: string }[]).map((tool) => tool.name).sort();
    expect(viaBridge.length).toBeGreaterThan(150);
    expect(viaBridge).toEqual(catalogue);
  });

  it('reads and writes through the pipe', async () => {
    const read = await bridge.callTool({ name: 'events_list', arguments: {} });
    expect(read.isError).toBeFalsy();

    const created = await bridge.callTool({
      name: 'events_create',
      arguments: {
        title: 'MCP stdio bridge event',
        date_time: new Date(Date.now() + 86_400_000).toISOString(),
      },
    });
    expect(created.isError).toBeFalsy();
    const [first] = created.content as { text: string }[];
    const event = JSON.parse(first?.text ?? '{}') as { event?: { id: string }; id?: string };
    const id = event.event?.id ?? event.id;
    expect(id).toBeTruthy();
    const removed = await bridge.callTool({ name: 'events_delete', arguments: { id } });
    expect(removed.isError).toBeFalsy();
  });

  it('forwards a resource notification from the server stream', async () => {
    const seen = new Promise<string>((resolve) => {
      bridge.setNotificationHandler(ResourceUpdatedNotificationSchema, (notification) => {
        resolve(notification.params.uri);
      });
    });
    await bridge.subscribeResource({ uri: 'metocast://state/events' });
    const made = await apiClient.post<{ id: string }>('/api/events', {
      title: 'MCP stdio bridge notification event',
      date_time: new Date(Date.now() + 86_400_000).toISOString(),
    });
    expect(made.status).toBeLessThan(300);
    const uri = await Promise.race([
      seen,
      new Promise<string>((_, reject) =>
        setTimeout(() => reject(new Error('no notification within 10 s')), 10_000),
      ),
    ]);
    expect(uri).toBe('metocast://state/events');
    await apiClient.delete(`/api/events/${made.body.id}`);
  });

  it('carries on when the server forgets its session', async () => {
    const before = await bridgeSessions();
    expect(before).toHaveLength(1);
    // What a Metocast restart (or fifteen idle minutes) does to the session.
    const gone = await fetch(`${BASE_URL}/mcp`, {
      method: 'DELETE',
      headers: { Authorization: `Bearer ${TOKEN}`, 'Mcp-Session-Id': before[0] ?? '' },
    });
    expect(gone.ok).toBe(true);
    expect(await bridgeSessions()).toHaveLength(0);

    // The AI app is not told anything: the next call just works, on a new session.
    const { tools } = await bridge.listTools();
    expect(tools.length).toBeGreaterThan(150);
    const after = await bridgeSessions();
    expect(after).toHaveLength(1);
    expect(after[0]).not.toBe(before[0]);
  });

  it('says how to fix it when MCP is turned off', async () => {
    await setConfig(false);
    try {
      await expect(bridge.callTool({ name: 'events_list', arguments: {} })).rejects.toThrow(
        /MCP is turned off\. Turn it on in the Metocast app/,
      );
    } finally {
      await setConfig(true);
    }
  });
});
