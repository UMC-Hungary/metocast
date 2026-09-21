import { z } from 'zod';
import { apiFetch } from './client.js';
import { sendWsCommand } from '../ws/client.js';
import { getAdminToken } from '../host/index.js';
import {
  AtemStreamTargetSchema,
  CameraSettingsSchema,
  CameraStreamTargetSchema,
  ConnectorConfigSchemas,
  DiscoveredCamerasSchema,
  DiscoveredMiddlecontrolsSchema,
  DiscoveredAtemsSchema,
  ConnectorStatusesSchema,
  ObsStreamSettingsSchema,
  type AtemStreamTarget,
  type ConnectorConfigMap,
  type ConnectorName,
  type CameraSettings,
  type CameraSettingsUpdate,
  type CameraStreamTarget,
  type DiscoveredCamera,
  type DiscoveredMiddlecontrol,
  type DiscoveredAtem,
  type ObsStreamSettings,
} from '../schemas/connectors.js';

export function fetchConnectorConfig<K extends ConnectorName>(
  name: K,
): Promise<ConnectorConfigMap[K]> {
  return apiFetch(`/api/connectors/${name}/config`, ConnectorConfigSchemas[name]) as Promise<
    ConnectorConfigMap[K]
  >;
}

export function saveConnectorConfig<K extends ConnectorName>(
  name: K,
  config: ConnectorConfigMap[K],
): Promise<void> {
  return apiFetch(`/api/connectors/${name}/config`, z.void(), { method: 'PUT', body: config });
}

/**
 * Reads a connector's stored secrets. Only works on the machine hosting the
 * server: the admin token comes from Tauri IPC and the core additionally
 * requires the request to arrive on loopback.
 */
export async function revealConnectorSecrets<K extends ConnectorName>(
  name: K,
): Promise<ConnectorConfigMap[K]> {
  const adminToken = await getAdminToken();
  return apiFetch(`/api/connectors/${name}/config/secrets`, ConnectorConfigSchemas[name], {
    headers: { 'X-Admin-Token': adminToken },
  }) as Promise<ConnectorConfigMap[K]>;
}

export function fetchConnectorStatuses(): Promise<z.infer<typeof ConnectorStatusesSchema>> {
  return apiFetch('/api/connectors/status', ConnectorStatusesSchema);
}

export function connectObs(): Promise<void> {
  return apiFetch('/api/connectors/obs/connect', z.void(), { method: 'POST' });
}

export function disconnectObs(): Promise<void> {
  return apiFetch('/api/connectors/obs/disconnect', z.void(), { method: 'POST' });
}

/** Starts the Blackmagic camera connector from the stored configuration. */
export function connectCamera(): Promise<void> {
  return apiFetch('/api/connectors/blackmagic-camera/connect', z.void(), { method: 'POST' });
}

/**
 * Scans the LAN for Blackmagic cameras. The core adopts and connects the first
 * one found when no camera is configured yet, so a scan is also how a camera
 * gets connected. Every client is told the result over `/ws` as well.
 */
export async function discoverCameras(): Promise<DiscoveredCamera[]> {
  const { cameras } = await apiFetch(
    '/api/connectors/blackmagic-camera/discover',
    DiscoveredCamerasSchema,
    {
      method: 'POST',
    },
  );
  return cameras;
}

/** Finds Middle Control by validating feedback on its known TCP ports. */
export async function discoverMiddlecontrol(): Promise<DiscoveredMiddlecontrol[]> {
  const { devices } = await apiFetch(
    '/api/connectors/middlecontrol/discover',
    DiscoveredMiddlecontrolsSchema,
    { method: 'POST' },
  );
  return devices;
}

/** Storage, record format and livestream settings, read from the camera in one pass. */
export function fetchCameraSettings(): Promise<CameraSettings> {
  return apiFetch('/api/connectors/blackmagic-camera/settings', CameraSettingsSchema);
}

/** Writes the record format, the livestream platform, or both, and reads the camera back. */
export function applyCameraSettings(update: CameraSettingsUpdate): Promise<CameraSettings> {
  return apiFetch('/api/connectors/blackmagic-camera/settings', CameraSettingsSchema, {
    method: 'PUT',
    body: update,
  });
}

/**
 * Copies the channel's RTMP ingestion address and stream key into the camera's
 * livestream settings. Sets the destination only — the camera does not go live.
 */
export function pushCameraYouTubeSettings(): Promise<CameraStreamTarget> {
  return apiFetch('/api/connectors/blackmagic-camera/stream/youtube', CameraStreamTargetSchema, {
    method: 'POST',
  });
}

export async function discoverAtem(): Promise<DiscoveredAtem[]> {
  const { devices } = await apiFetch('/api/connectors/atem/discover', DiscoveredAtemsSchema, {
    method: 'POST',
  });
  return devices;
}

export function pushAtemYouTubeSettings(): Promise<AtemStreamTarget> {
  return apiFetch('/api/connectors/atem/stream/youtube', AtemStreamTargetSchema, {
    method: 'POST',
  });
}

/**
 * Sets a RØDECaster channel's mute. The value is absolute, so re-sending the same
 * state is safe; the resulting state arrives as a `rodecaster.mute` event from the
 * device, not from this call.
 */
export function setRodecasterMute(channel: number, mute: boolean): boolean {
  return sendWsCommand('rodecaster.mute.set', { channel, mute });
}

/** Asks the core for the current channel profile, answered as `rodecaster.profile`. */
export function requestRodecasterProfile(): boolean {
  return sendWsCommand('rodecaster.profile');
}

export function discoverRodecasterAudio(): boolean {
  return sendWsCommand('rodecaster.audio.discover');
}

export function requestRodecasterAudioRecordingState(): boolean {
  return sendWsCommand('rodecaster.audio.record.state');
}

export function startRodecasterAudioRecording(eventId: string): boolean {
  return sendWsCommand('rodecaster.audio.record.start', { event_id: eventId });
}

export function stopRodecasterAudioRecording(): boolean {
  return sendWsCommand('rodecaster.audio.record.stop');
}

export function selectMiddlecontrolCamera(cameraId: number): boolean {
  return sendWsCommand('middlecontrol.camera.select', { camera_id: cameraId });
}

export function startMiddlecontrolRecording(cameraId?: number): boolean {
  return sendWsCommand(
    'middlecontrol.record.start',
    cameraId === undefined ? undefined : { camera_id: cameraId },
  );
}

export function stopMiddlecontrolRecording(cameraId?: number): boolean {
  return sendWsCommand(
    'middlecontrol.record.stop',
    cameraId === undefined ? undefined : { camera_id: cameraId },
  );
}

export function startAllMiddlecontrolRecordings(): boolean {
  return sendWsCommand('middlecontrol.record.start_all');
}

export function stopAllMiddlecontrolRecordings(): boolean {
  return sendWsCommand('middlecontrol.record.stop_all');
}

export function recallMiddlecontrolPreset(preset: number, cameraId?: number): boolean {
  return sendWsCommand(
    'middlecontrol.preset.recall',
    cameraId === undefined ? { preset } : { preset, camera_id: cameraId },
  );
}

export function fetchObsStreamSettings(): Promise<ObsStreamSettings> {
  return apiFetch('/api/connectors/obs/stream-settings', ObsStreamSettingsSchema);
}

export function applyObsStreamSettings(server: string, key: string): Promise<void> {
  return apiFetch('/api/connectors/obs/stream-settings', z.void(), {
    method: 'PUT',
    body: { server, key },
  });
}

const ChannelVideoItemSchema = z.object({
  id: z.string(),
  title: z.string(),
  thumbnailUrl: z.string(),
  publishedAt: z.string().nullable().optional(),
  viewCount: z.number().nullable().optional(),
  likeCount: z.number().nullable().optional(),
  duration: z.string().nullable().optional(),
  liveStatus: z.string(),
  scheduledStartTime: z.string().nullable().optional(),
  watchUrl: z.string(),
  privacyStatus: z.string().default('public'),
});

export const ChannelContentSchema = z.object({
  liveBroadcasts: z.array(ChannelVideoItemSchema),
  videos: z.array(ChannelVideoItemSchema),
});

export type ChannelVideoItem = z.infer<typeof ChannelVideoItemSchema>;
export type ChannelContent = z.infer<typeof ChannelContentSchema>;

const AuthUrlSchema = z.object({ url: z.string() });

export async function youtubeAuthUrl(): Promise<string> {
  return (await apiFetch('/api/auth/youtube/url', AuthUrlSchema)).url;
}

export async function facebookAuthUrl(): Promise<string> {
  return (await apiFetch('/api/auth/facebook/url', AuthUrlSchema)).url;
}

export function youtubeLogout(): Promise<void> {
  return apiFetch('/api/auth/youtube/logout', z.void(), { method: 'POST' });
}

export function facebookLogout(): Promise<void> {
  return apiFetch('/api/auth/facebook/logout', z.void(), { method: 'POST' });
}

export function triggerYouTubeSchedule(eventId: string): Promise<void> {
  return apiFetch(`/api/connectors/youtube/schedule/${eventId}`, z.void(), { method: 'POST' });
}

export function triggerFacebookSchedule(eventId: string): Promise<void> {
  return apiFetch(`/api/connectors/facebook/schedule/${eventId}`, z.void(), { method: 'POST' });
}

export function fetchYouTubeContent(): Promise<ChannelContent> {
  return apiFetch('/api/connectors/youtube/content', ChannelContentSchema);
}

const StreamKeySchema = z.object({ rtmpUrl: z.string() });

export function fetchYouTubeStreamKey(): Promise<{ rtmpUrl: string }> {
  return apiFetch('/api/connectors/youtube/stream-key', StreamKeySchema);
}

export function fetchFacebookStreamKey(): Promise<{ rtmpUrl: string }> {
  return apiFetch('/api/connectors/facebook/stream-key', StreamKeySchema);
}
