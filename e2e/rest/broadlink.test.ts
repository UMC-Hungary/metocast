/**
 * E2E tests for Broadlink REST API.
 */

import { describe, it, expect } from 'vitest';
import { apiClient } from '../helpers/client.js';

interface BroadlinkDevice {
  id: string;
  name: string;
  deviceType: string;
  model: string | null;
  host: string;
  mac: string;
  isDefault: boolean;
}

interface BroadlinkCommand {
  id: string;
  deviceId: string | null;
  name: string;
  slug: string;
  code: string;
  codeType: string;
  category: string;
}

const isLive = !!process.env.TAURI_TEST_TOKEN;

describe.skipIf(!isLive)('Broadlink REST API', () => {
  let deviceId: string;
  let commandId: string;

  it('GET /api/connectors/broadlink/status → 200', async () => {
    const res = await apiClient.get<{ status: { type: string } }>(
      '/api/connectors/broadlink/status',
    );
    expect(res.status).toBe(200);
    expect(res.body).toHaveProperty('status');
  });

  it('GET /api/connectors/broadlink/devices → 200 with array', async () => {
    const res = await apiClient.get<BroadlinkDevice[]>('/api/connectors/broadlink/devices');
    expect(res.status).toBe(200);
    expect(Array.isArray(res.body)).toBe(true);
  });

  it('POST /api/connectors/broadlink/devices → 201', async () => {
    const res = await apiClient.post<BroadlinkDevice>('/api/connectors/broadlink/devices', {
      name: 'E2E Test Device',
      host: '192.168.1.200',
      mac: 'AA:BB:CC:DD:EE:01',
      deviceType: 'RM4Pro',
      model: 'RM4 Pro',
    });
    expect(res.status).toBe(201);
    expect(res.body).toHaveProperty('id');
    deviceId = res.body.id;
  });

  it('GET /api/connectors/broadlink/commands → 200 with array', async () => {
    const res = await apiClient.get<BroadlinkCommand[]>('/api/connectors/broadlink/commands');
    expect(res.status).toBe(200);
    expect(Array.isArray(res.body)).toBe(true);
  });

  it('POST /api/connectors/broadlink/commands → 201', async () => {
    const res = await apiClient.post<BroadlinkCommand>('/api/connectors/broadlink/commands', {
      name: 'E2E Power On',
      slug: 'e2e_power_on',
      code: 'JgBGAAABJ',
      codeType: 'ir',
      category: 'power',
    });
    expect(res.status).toBe(201);
    expect(res.body).toHaveProperty('id');
    commandId = res.body.id;
  });

  it('PUT /api/connectors/broadlink/commands/:id → 204', async () => {
    expect(commandId).toBeDefined();
    const res = await apiClient.put(`/api/connectors/broadlink/commands/${commandId}`, {
      name: 'E2E Power On (updated)',
    });
    expect(res.status).toBe(204);
  });

  it('DELETE /api/connectors/broadlink/commands/:id → 204', async () => {
    expect(commandId).toBeDefined();
    const res = await apiClient.delete(`/api/connectors/broadlink/commands/${commandId}`);
    expect(res.status).toBe(204);
  });

  it('DELETE /api/connectors/broadlink/devices/:id → 204', async () => {
    expect(deviceId).toBeDefined();
    const res = await apiClient.delete(`/api/connectors/broadlink/devices/${deviceId}`);
    expect(res.status).toBe(204);
  });
});
