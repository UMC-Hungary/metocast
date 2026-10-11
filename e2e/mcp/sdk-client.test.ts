/**
 * The same server, driven by the official MCP SDK client over Streamable HTTP, so the protocol is
 * checked against an implementation we did not write: initialize, tools, resources, prompts, and
 * a subscription whose notification arrives over the server-sent stream.
 */

import { afterAll, beforeAll, describe, expect, it } from 'vitest';
import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { StreamableHTTPClientTransport } from '@modelcontextprotocol/sdk/client/streamableHttp.js';
import { ResourceUpdatedNotificationSchema } from '@modelcontextprotocol/sdk/types.js';
import { apiClient } from '../helpers/client';

const isLive = !!process.env.TAURI_TEST_TOKEN;
const BASE_URL = process.env.TAURI_TEST_BASE_URL ?? 'http://localhost:3738';
const TOKEN = process.env.TAURI_TEST_TOKEN ?? '';

describe.skipIf(!isLive)('MCP server with the official SDK client', () => {
  let client: Client;

  beforeAll(async () => {
    await apiClient.put('/api/connectors/mcp/config', {
      enabled: true,
      readOnly: false,
      allowLan: false,
    });
    client = new Client({ name: 'sdk-e2e', version: '1.0.0' });
    await client.connect(
      new StreamableHTTPClientTransport(new URL(`${BASE_URL}/mcp`), {
        requestInit: { headers: { Authorization: `Bearer ${TOKEN}` } },
      }),
    );
  });

  afterAll(async () => {
    await client.close();
    await apiClient.put('/api/connectors/mcp/config', {
      enabled: false,
      readOnly: false,
      allowLan: false,
    });
  });

  it('negotiates and advertises tools, resources and prompts', () => {
    expect(client.getServerVersion()?.name).toBe('metocast');
    const capabilities = client.getServerCapabilities();
    expect(capabilities?.tools).toBeTruthy();
    expect(capabilities?.resources?.subscribe).toBe(true);
    expect(capabilities?.prompts).toBeTruthy();
  });

  it('lists tools and calls one', async () => {
    const { tools } = await client.listTools();
    expect(tools.length).toBeGreaterThan(150);
    const listEvents = tools.find((tool) => tool.name === 'events_list');
    expect(listEvents?.annotations?.readOnlyHint).toBe(true);
    const result = await client.callTool({ name: 'events_list', arguments: {} });
    expect(result.isError).toBeFalsy();
  });

  it('does not offer the stream keys', async () => {
    const { tools } = await client.listTools();
    const names = tools.map((tool) => tool.name);
    expect(names).not.toContain('connectors_youtube_stream_key');
    expect(names).not.toContain('connectors_facebook_stream_key');
  });

  it('serves state and the manual as resources', async () => {
    const { resources } = await client.listResources();
    const uris = resources.map((resource) => resource.uri);
    expect(uris).toContain('metocast://state/connectors');
    expect(uris).toContain('metocast://state/transcription');
    expect(uris).toContain('metocast://docs/features/media.md');

    const state = await client.readResource({ uri: 'metocast://state/queues' });
    expect(state.contents[0]?.mimeType).toBe('application/json');
    const feature = await client.readResource({
      uri: 'metocast://docs/features/connector-automations.md',
    });
    expect(String(feature.contents[0]?.text)).toContain('Connector automations');

    const { resourceTemplates } = await client.listResourceTemplates();
    expect(resourceTemplates.map((t) => t.uriTemplate)).toContain('metocast://state/events/{id}');
    await expect(client.readResource({ uri: 'metocast://state/nope' })).rejects.toThrow();
  });

  it('serves prompts that name real tools', async () => {
    const { prompts } = await client.listPrompts();
    expect(prompts.map((prompt) => prompt.name)).toEqual(
      expect.arrayContaining(['prepare_service', 'go_live_checklist', 'post_service_upload']),
    );
    const prompt = await client.getPrompt({
      name: 'post_service_upload',
      arguments: { event_id: 'abc' },
    });
    const first = prompt.messages[0]?.content;
    expect(first && 'text' in first ? first.text : '').toContain('event abc');
    await expect(client.getPrompt({ name: 'post_service_upload' })).rejects.toThrow();
  });

  it('notifies a subscriber when a resource it holds changes', async () => {
    const seen = new Promise<string>((resolve) => {
      client.setNotificationHandler(ResourceUpdatedNotificationSchema, (notification) => {
        if (notification.params.uri === 'metocast://state/events') {
          resolve(notification.params.uri);
        }
      });
    });
    await client.subscribeResource({ uri: 'metocast://state/events' });
    const made = await apiClient.post<{ id: string }>('/api/events', {
      title: 'MCP subscription event',
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
    await client.unsubscribeResource({ uri: 'metocast://state/events' });
    await apiClient.delete(`/api/events/${made.body.id}`);
  });
});
