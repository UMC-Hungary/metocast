import { writable } from 'svelte/store';
import type { ConnectorState } from '$lib/connectors/types.js';
import type { ConnectorConfigMap } from '@metocast/core-client/schemas/connectors';

export type ConnectorStatus = 'disconnected' | 'connecting' | 'connected' | 'error';

export type ObsConfig = ConnectorConfigMap['obs'];
export type VmixConfig = ConnectorConfigMap['vmix'];
export type AtemConfig = ConnectorConfigMap['atem'];
export type MiddlecontrolConfig = ConnectorConfigMap['middlecontrol'];
export type BroadlinkConfig = ConnectorConfigMap['broadlink'];
export type YouTubeConfig = ConnectorConfigMap['youtube'];
export type FacebookConfig = ConnectorConfigMap['facebook'];
export type DiscordConfig = ConnectorConfigMap['discord'];
export type SzentirasConfig = ConnectorConfigMap['szentiras'];

/** Payload shape emitted by the Rust backend. */
export interface ConnectorStatusPayload {
  type: ConnectorStatus;
  message?: string | undefined;
}

/** Map the tagged-union payload from Rust to the simple status string. */
export function mapConnectorStatus(payload: ConnectorStatusPayload): ConnectorStatus {
  return payload.type;
}

// ── Status stores (source of truth for ConnectorStatusBadge) ─────────────────

export const obsStatus = writable<ConnectorStatus>('disconnected');
export const vmixStatus = writable<ConnectorStatus>('disconnected');
export const atemStatus = writable<ConnectorStatus>('disconnected');
export const middlecontrolStatus = writable<ConnectorStatus>('disconnected');
export const broadlinkStatus = writable<ConnectorStatus>('disconnected');
export const youtubeStatus = writable<ConnectorStatus>('disconnected');
export const facebookStatus = writable<ConnectorStatus>('disconnected');
export const discordStatus = writable<ConnectorStatus>('disconnected');
export const szentirasStatus = writable<ConnectorStatus>('disconnected');

// ── Config stores ─────────────────────────────────────────────────────────────

export const obsConfig = writable<ObsConfig>({
  enabled: false,
  host: 'localhost',
  port: 4455,
  password: null,
});

export const vmixConfig = writable<VmixConfig>({
  enabled: false,
  host: 'localhost',
  port: 8088,
});

export const atemConfig = writable<AtemConfig>({
  enabled: false,
  host: '',
  port: 9910,
});

export const middlecontrolConfig = writable<MiddlecontrolConfig>({
  enabled: false,
  host: '',
  port: 11584,
});

export const broadlinkConfig = writable<BroadlinkConfig>({
  enabled: false,
});

export const youtubeConfig = writable<YouTubeConfig>({
  enabled: false,
  clientId: '',
  clientSecret: '',
});

export const facebookConfig = writable<FacebookConfig>({
  enabled: false,
  appId: '',
  appSecret: '',
  pageId: '',
});

export const discordConfig = writable<DiscordConfig>({
  enabled: false,
  webhookUrl: '',
});

export const szentirasConfig = writable<SzentirasConfig>({
  enabled: false,
  apiKey: '',
});

// ── Extended state stores (status + boolean capability flags) ─────────────────
// These are updated alongside the status stores. The boolean flags (isStreaming,
// isRecording, isLive) will be populated when WS messages for them arrive.

export const obsState = writable<ConnectorState>({
  connection: 'disconnected',
  isStreaming: false,
  isRecording: false,
});
export const vmixState = writable<ConnectorState>({ connection: 'disconnected' });
export const atemState = writable<ConnectorState>({ connection: 'disconnected' });
export const middlecontrolState = writable<ConnectorState>({ connection: 'disconnected' });
export const broadlinkState = writable<ConnectorState>({ connection: 'disconnected' });
export const youtubeState = writable<ConnectorState>({ connection: 'disconnected' });
export const facebookState = writable<ConnectorState>({ connection: 'disconnected' });
export const discordState = writable<ConnectorState>({ connection: 'disconnected' });
export const szentirasState = writable<ConnectorState>({ connection: 'disconnected' });

/** True when the cron.youtube_pull message reports at least one live broadcast. */
export const youtubeLiveActive = writable<boolean>(false);

/** Status and extended-state stores keyed by connector id. */
export const connectorStores = {
  obs: { status: obsStatus, state: obsState },
  vmix: { status: vmixStatus, state: vmixState },
  atem: { status: atemStatus, state: atemState },
  middlecontrol: { status: middlecontrolStatus, state: middlecontrolState },
  broadlink: { status: broadlinkStatus, state: broadlinkState },
  youtube: { status: youtubeStatus, state: youtubeState },
  facebook: { status: facebookStatus, state: facebookState },
  discord: { status: discordStatus, state: discordState },
  szentiras: { status: szentirasStatus, state: szentirasState },
} as const;

/** Applies a `/api/connectors/status` payload to the stores. */
export function applyConnectorStatuses(
  statuses: Partial<Record<keyof typeof connectorStores, ConnectorStatusPayload>>,
): void {
  for (const [name, entry] of Object.entries(connectorStores)) {
    const payload = statuses[name as keyof typeof connectorStores];
    if (!payload) continue;
    const mapped = mapConnectorStatus(payload);
    entry.status.set(mapped);
    entry.state.update((s) => ({ ...s, connection: mapped }));
  }
}
