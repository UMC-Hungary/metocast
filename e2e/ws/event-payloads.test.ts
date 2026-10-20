/**
 * The push messages a live server really sends, checked against the catalogue in
 * `server/ws_events.rs` (as `docs/api/ws-events.schema.json`). A field the server starts sending
 * without declaring it, or stops sending, fails here.
 */

import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { afterAll, beforeAll, describe, expect, it } from 'vitest';
import WebSocket from 'ws';
import { apiClient } from '../helpers/client';

const isLive = !!process.env.TAURI_TEST_TOKEN;
const WS_URL = process.env.TAURI_TEST_WS_URL ?? 'ws://localhost:3738';
const TOKEN = process.env.TAURI_TEST_TOKEN ?? '';

type Schema = {
  properties: Record<string, { type?: string; anyOf?: { type?: string }[] }>;
  required: string[];
};

const schemas = JSON.parse(
  readFileSync(join(process.cwd(), 'docs/api/ws-events.schema.json'), 'utf8'),
) as Record<string, Schema>;

const kindOf = (value: unknown): string =>
  value === null
    ? 'null'
    : Array.isArray(value)
      ? 'array'
      : typeof value === 'object'
        ? 'object'
        : typeof value;

/** What is wrong with one message, or nothing. */
function problems(message: Record<string, unknown>): string[] {
  const type = String(message.type);
  const schema = schemas[type];
  if (!schema) return [];
  const found: string[] = [];
  for (const key of Object.keys(message)) {
    if (!(key in schema.properties)) found.push(`${type}: undeclared field "${key}"`);
  }
  for (const key of schema.required) {
    if (!(key in message)) found.push(`${type}: missing "${key}"`);
  }
  for (const [key, spec] of Object.entries(schema.properties)) {
    if (!(key in message) || key === 'type') continue;
    const allowed = spec.anyOf ? spec.anyOf.map((s) => s.type) : [spec.type];
    const kind = kindOf(message[key]);
    const matches = allowed.some(
      (t) =>
        t === undefined ||
        t === kind ||
        (t === 'integer' && Number.isInteger(message[key])) ||
        (t === 'number' && kind === 'number'),
    );
    if (!matches) found.push(`${type}.${key}: ${kind} where ${allowed.join(' or ')} is declared`);
  }
  return found;
}

describe.skipIf(!isLive)('Push message payloads', () => {
  const seen: Record<string, unknown>[] = [];
  let socket: WebSocket;
  let eventId = '';
  let folderId = '';

  beforeAll(async () => {
    socket = new WebSocket(`${WS_URL}/ws?token=${TOKEN}`);
    socket.on('message', (data) => {
      try {
        seen.push(JSON.parse(data.toString()) as Record<string, unknown>);
      } catch {
        /* not JSON */
      }
    });
    await new Promise<void>((resolve, reject) => {
      socket.once('open', () => resolve());
      socket.once('error', reject);
    });
    await new Promise((resolve) => setTimeout(resolve, 1500));

    // Make the server push the changes the database and the folders announce.
    const event = await apiClient.post<{ id: string }>('/api/events', {
      title: 'Payload shape event',
      date_time: new Date(Date.now() + 86_400_000).toISOString(),
    });
    eventId = event.body.id;
    await apiClient.post(`/api/events/${eventId}/recordings`, {
      file_path: '/tmp/payload-shape.mp4',
      file_name: 'payload-shape.mp4',
      file_size: 1024,
      duration_seconds: 1,
    });
    const folder = await apiClient.post<{ data: { id: string } }>('/api/ppt/folders', {
      path: '/tmp',
      name: 'Payload shape folder',
    });
    folderId = folder.body.data?.id ?? '';
    await new Promise((resolve) => setTimeout(resolve, 2000));
  });

  afterAll(async () => {
    socket.close();
    if (folderId) await apiClient.delete(`/api/ppt/folders/${folderId}`);
    if (eventId) await apiClient.delete(`/api/events/${eventId}`);
  });

  it('declares every field the server sends, and sends every field it declares', () => {
    const all = seen.flatMap((message) => problems(message));
    expect([...new Set(all)]).toEqual([]);
  });

  it('exercised the pushes a clean server can produce', () => {
    const types = new Set(seen.map((message) => String(message.type)));
    for (const expected of [
      'connected',
      'clients.updated',
      'connector.status',
      'presentation.settings',
      'presenter.state',
      'transcription.snapshot',
      'automations.changed',
      'event.changed',
      'recording.changed',
      'ppt.folders_changed',
      'ppt.index.state',
    ]) {
      expect(types, `no ${expected} was seen`).toContain(expected);
    }
  });
});
