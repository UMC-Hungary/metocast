/**
 * Binds the framework-agnostic WebSocket transport to this UI's stores and
 * notifications. The transport (in @metocast/core-client) validates and emits
 * typed messages; everything here — which store to write, which toast to raise —
 * is classic's, and would differ in any other UI.
 */
import {
  connectWs as transportConnect,
  disconnectWs as transportDisconnect,
  sendWsCommand,
  listFolders,
} from '@metocast/core-client';
import type { WsMessage } from '@metocast/core-client';
import type { EventSummary } from '@metocast/core-client/schemas/event';
import { get } from 'svelte/store';
import { wsStatus, lastWsMessage } from '$lib/stores/ws.js';
import { events, untrackedRecordings } from '$lib/stores/events.js';
import {
  obsStatus,
  obsState,
  vmixStatus,
  vmixState,
  atemStatus,
  atemState,
  middlecontrolStatus,
  middlecontrolState,
  broadlinkStatus,
  broadlinkState,
  youtubeStatus,
  youtubeState,
  facebookStatus,
  facebookState,
  discordStatus,
  discordState,
  youtubeLiveActive,
} from '$lib/stores/connectors.js';
import { broadlinkDiscoveredDevices, broadlinkLearnResult } from '$lib/stores/broadlink.js';
import { keynoteStatus, pptResults, pptFolders, pptFilter } from '$lib/stores/presentations.js';
import {
  presenterState,
  presenterTheme,
  connectedClients,
  useWebPresenter,
} from '$lib/stores/presenter.js';
import { uploadProgress } from '$lib/stores/uploads.js';
import { queues } from '$lib/stores/queues.js';
import {
  handleObsDevicesMessage,
  obsDeviceListeners,
  obsDeviceListenerStatuses,
} from '$lib/stores/obs-devices.js';
import { pushError, clearErrors } from '$lib/stores/errors.js';
import { toast } from 'svelte-sonner';

export function connectWs(): void {
  transportConnect({
    onStatus: (status) => wsStatus.set(status),
    onOpen: () => {
      sendWsCommand('presenter.register', { label: 'Tauri App' });
    },
    onMessage: (msg) => {
      lastWsMessage.set(msg);
      handleMessage(msg);
    },
  });
}

export function disconnectWs(): void {
  transportDisconnect();
}

function handleMessage(msg: WsMessage): void {
  if (msg.type === 'error') {
    toast.error(msg.message);
  } else if (msg.type === 'notification') {
    if (msg.level === 'warn') toast.warning(msg.message);
    else if (msg.level === 'error') toast.error(msg.message);
    else toast.info(msg.message);
  } else if (msg.type === 'event.changed') {
    const { operation, record: rec } = msg.data;
    if (operation === 'DELETE') {
      events.update((list) => list.filter((e) => e.id !== rec.id));
    } else {
      events.update((list) => {
        const idx = list.findIndex((e) => e.id === rec.id);
        const existing = idx >= 0 ? list[idx] : undefined;
        const summary: EventSummary = {
          id: rec.id,
          title: rec.title,
          computedTitle: rec.computedTitle,
          dateTime: rec.dateTime,
          speaker: rec.speaker,
          recordingCount: existing?.recordingCount ?? 0,
          isCompleted: existing?.isCompleted ?? false,
          createdAt: rec.createdAt,
          updatedAt: rec.updatedAt,
        };
        if (idx >= 0) {
          const updated = [...list];
          updated[idx] = { ...updated[idx], ...summary };
          return updated;
        }
        return [summary, ...list];
      });
      if (operation === 'INSERT') {
        toast.success(`New event created: ${rec.title}`);
      } else if (operation === 'UPDATE') {
        toast.info(`Event updated: ${rec.title}`);
      }
    }
  } else if (msg.type === 'recording.detected') {
    if (msg.eventTitle !== null) {
      toast.success(`Recording added to ${msg.eventTitle}`);
    } else {
      toast.info('New untracked recording detected');
    }
  } else if (msg.type === 'recording.untracked.removed') {
    untrackedRecordings.update((list) => list.filter((r) => r.id !== msg.id));
  } else if (msg.type === 'obs.state') {
    obsState.update((s) => ({ ...s, isStreaming: msg.isStreaming, isRecording: msg.isRecording }));
  } else if (msg.type === 'connector.status') {
    const status = msg.status.type;
    if (msg.connector === 'obs') {
      obsStatus.set(status);
      const resetState = status === 'disconnected' || status === 'error';
      obsState.update((s) => ({
        ...s,
        connection: status,
        isStreaming: resetState ? false : (s.isStreaming ?? false),
        isRecording: resetState ? false : (s.isRecording ?? false),
      }));
    } else if (msg.connector === 'vmix') {
      vmixStatus.set(status);
      vmixState.update((s) => ({ ...s, connection: status }));
    } else if (msg.connector === 'atem') {
      atemStatus.set(status);
      atemState.update((s) => ({ ...s, connection: status }));
    } else if (msg.connector === 'middlecontrol') {
      middlecontrolStatus.set(status);
      middlecontrolState.update((s) => ({ ...s, connection: status }));
    } else if (msg.connector === 'broadlink') {
      broadlinkStatus.set(status);
      broadlinkState.update((s) => ({ ...s, connection: status }));
    } else if (msg.connector === 'youtube') {
      youtubeStatus.set(status);
      youtubeState.update((s) => ({ ...s, connection: status }));
    } else if (msg.connector === 'facebook') {
      facebookStatus.set(status);
      facebookState.update((s) => ({ ...s, connection: status }));
    } else if (msg.connector === 'discord') {
      discordStatus.set(status);
      discordState.update((s) => ({ ...s, connection: status }));
    }
  } else if (msg.type === 'connector.state' && msg.connector !== 'atem') {
    const patch: { isStreaming?: boolean; isRecording?: boolean } = {};
    if (msg.isStreaming !== undefined) patch.isStreaming = msg.isStreaming;
    if (msg.isRecording !== undefined) patch.isRecording = msg.isRecording;
    if (msg.connector === 'obs') {
      obsState.update((s) => ({ ...s, ...patch }));
    } else if (msg.connector === 'vmix') {
      vmixState.update((s) => ({ ...s, ...patch }));
    } else if (msg.connector === 'broadlink') {
      broadlinkState.update((s) => ({ ...s, ...patch }));
    } else if (msg.connector === 'middlecontrol' && msg.state) {
      middlecontrolState.update((s) => ({ ...s, ...msg.state }));
    }
  } else if (msg.type === 'broadlink.device.discovered') {
    broadlinkDiscoveredDevices.update((list) => {
      const exists = list.some((d) => d.mac === msg.device.mac);
      return exists ? list : [...list, msg.device];
    });
  } else if (msg.type === 'broadlink.learn.result') {
    broadlinkLearnResult.set({ code: msg.code, error: msg.error });
  } else if (msg.type === 'cron.youtube_pull') {
    youtubeLiveActive.set(msg.hasLive);
    youtubeState.update((s) => ({ ...s, isLive: msg.hasLive }));
  } else if (msg.type === 'presentation.settings') {
    useWebPresenter.set(msg.useWebPresenter);
    presenterTheme.set(msg.presenterTheme);
  } else if (msg.type === 'presenter.state') {
    presenterState.set(msg.state);
  } else if (msg.type === 'presenter.slide_changed') {
    presenterState.update((s) => ({
      ...s,
      currentSlide: msg.currentSlide,
      totalSlides: msg.totalSlides,
    }));
  } else if (msg.type === 'clients.updated' || msg.type === 'clients.list') {
    connectedClients.set(msg.clients);
  } else if (msg.type === 'ping') {
    sendWsCommand('pong', { ping_id: msg.pingId });
  } else if (msg.type === 'keynote.status') {
    keynoteStatus.set(msg.status);
  } else if (msg.type === 'ppt.search_results') {
    pptResults.set(msg.files);
    if (msg.filter !== undefined) pptFilter.set(msg.filter);
  } else if (msg.type === 'ppt.folders_changed') {
    listFolders().then((folders) => pptFolders.set(folders));
  } else if (msg.type === 'upload.progress') {
    uploadProgress.update((map) => ({
      ...map,
      [`${msg.recordingId}:${msg.platform}`]: {
        platform: msg.platform,
        progressBytes: msg.progressBytes,
        totalBytes: msg.totalBytes,
        state: 'uploading' as const,
      },
    }));
  } else if (msg.type === 'upload.completed') {
    uploadProgress.update((map) => ({
      ...map,
      [`${msg.recordingId}:${msg.platform}`]: {
        platform: msg.platform,
        progressBytes: 0,
        totalBytes: 0,
        state: 'completed' as const,
        videoId: msg.videoId,
        videoUrl: msg.videoUrl,
      },
    }));
    toast.success(`Recording uploaded to ${msg.platform}`);
  } else if (msg.type === 'upload.failed') {
    uploadProgress.update((map) => ({
      ...map,
      [`${msg.recordingId}:${msg.platform}`]: {
        platform: msg.platform,
        progressBytes: 0,
        totalBytes: 0,
        state: 'failed' as const,
        error: msg.error,
      },
    }));
    toast.error(`Upload to ${msg.platform} failed: ${msg.error}`);
  } else if (msg.type === 'upload.paused') {
    uploadProgress.update((map) => {
      const updated = { ...map };
      for (const key of Object.keys(updated)) {
        if (key.startsWith(`${msg.recordingId}:`)) {
          const existing = updated[key];
          if (existing) {
            updated[key] = { ...existing, state: 'paused' as const };
          }
        }
      }
      return updated;
    });
  } else if (
    msg.type === 'obs.devices.available' ||
    msg.type === 'obs.listeners.list' ||
    msg.type === 'obs.listeners.create' ||
    msg.type === 'obs.listeners.update' ||
    msg.type === 'obs.listeners.delete'
  ) {
    handleObsDevicesMessage(msg);
    syncDeviceListenerErrors();
  } else if (msg.type === 'queue.stats') {
    queues.set(msg.queues);
  }
}

function syncDeviceListenerErrors(): void {
  const listeners = get(obsDeviceListeners);
  const statuses = get(obsDeviceListenerStatuses);
  for (const listener of listeners) {
    const connectorId = `device:${listener.id}`;
    const status = statuses.find((s) => s.listenerId === listener.id);
    if (status && !status.available) {
      pushError({
        connectorId,
        connectorName: listener.friendlyName,
        message: 'Device unavailable',
      });
    } else {
      clearErrors(connectorId);
    }
  }
}
