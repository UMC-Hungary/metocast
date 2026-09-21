import { coreConfig } from '../config.js';

async function apiFetch(path: string, options: RequestInit = {}): Promise<Response> {
  const { serverUrl: base, authToken: token } = coreConfig();
  return fetch(`${base}${path}`, {
    ...options,
    headers: {
      'Content-Type': 'application/json',
      Authorization: `Bearer ${token}`,
      ...options.headers,
    },
  });
}

export interface BroadlinkDevice {
  id: string;
  name: string;
  deviceType: string;
  model: string | null;
  host: string;
  mac: string;
  isDefault: boolean;
}

export interface BroadlinkCommand {
  id: string;
  deviceId: string | null;
  name: string;
  slug: string;
  code: string;
  codeType: string;
  category: string;
}

export async function fetchDevices(): Promise<BroadlinkDevice[]> {
  const res = await apiFetch('/api/connectors/broadlink/devices');
  if (!res.ok) throw new Error(`Failed to fetch devices: ${res.status}`);
  return res.json() as Promise<BroadlinkDevice[]>;
}

export async function addDevice(body: {
  name: string;
  host: string;
  mac: string;
  deviceType: string;
  model?: string;
}): Promise<BroadlinkDevice> {
  const res = await apiFetch('/api/connectors/broadlink/devices', {
    method: 'POST',
    body: JSON.stringify(body),
  });
  if (!res.ok) throw new Error(`Failed to add device: ${res.status}`);
  return res.json() as Promise<BroadlinkDevice>;
}

export async function removeDevice(id: string): Promise<void> {
  const res = await apiFetch(`/api/connectors/broadlink/devices/${id}`, { method: 'DELETE' });
  if (!res.ok) throw new Error(`Failed to remove device: ${res.status}`);
}

export async function triggerDiscover(): Promise<void> {
  const res = await apiFetch('/api/connectors/broadlink/discover', { method: 'POST' });
  if (!res.ok) throw new Error(`Failed to start discovery: ${res.status}`);
}

export async function fetchCommands(
  deviceId?: string,
  category?: string,
): Promise<BroadlinkCommand[]> {
  const params = new URLSearchParams();
  if (deviceId) params.set('device_id', deviceId);
  if (category) params.set('category', category);
  const qs = params.size > 0 ? `?${params.toString()}` : '';
  const res = await apiFetch(`/api/connectors/broadlink/commands${qs}`);
  if (!res.ok) throw new Error(`Failed to fetch commands: ${res.status}`);
  return res.json() as Promise<BroadlinkCommand[]>;
}

export async function addCommand(body: {
  deviceId?: string;
  name: string;
  slug: string;
  code: string;
  codeType: string;
  category?: string;
}): Promise<BroadlinkCommand> {
  const res = await apiFetch('/api/connectors/broadlink/commands', {
    method: 'POST',
    body: JSON.stringify(body),
  });
  if (!res.ok) throw new Error(`Failed to add command: ${res.status}`);
  return res.json() as Promise<BroadlinkCommand>;
}

export async function updateCommand(
  id: string,
  body: { name?: string; slug?: string; code?: string; codeType?: string; category?: string },
): Promise<void> {
  const res = await apiFetch(`/api/connectors/broadlink/commands/${id}`, {
    method: 'PUT',
    body: JSON.stringify(body),
  });
  if (!res.ok) throw new Error(`Failed to update command: ${res.status}`);
}

export async function removeCommand(id: string): Promise<void> {
  const res = await apiFetch(`/api/connectors/broadlink/commands/${id}`, { method: 'DELETE' });
  if (!res.ok) throw new Error(`Failed to remove command: ${res.status}`);
}

export async function startLearn(deviceId: string, signalType = 'ir'): Promise<void> {
  const res = await apiFetch(`/api/connectors/broadlink/devices/${deviceId}/learn`, {
    method: 'POST',
    body: JSON.stringify({ signalType }),
  });
  if (!res.ok) {
    const body = (await res.json().catch(() => ({}))) as { error?: string };
    throw new Error(body.error ?? `Failed to start learn: ${res.status}`);
  }
}

export async function cancelLearn(): Promise<void> {
  const res = await apiFetch('/api/connectors/broadlink/learn/cancel', { method: 'POST' });
  if (!res.ok) throw new Error(`Failed to cancel learn: ${res.status}`);
}

export async function sendCommand(id: string): Promise<void> {
  const res = await apiFetch(`/api/connectors/broadlink/commands/${id}/send`, { method: 'POST' });
  if (!res.ok) throw new Error(`Failed to send command: ${res.status}`);
}
