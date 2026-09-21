import { writable } from 'svelte/store';
import type {
  ObsAvailableDevices,
  DeviceListener,
  DeviceListenerStatus,
} from '@metocast/core-client/schemas/ws-messages';
import type { WsMessage } from '@metocast/core-client/schemas/ws-messages';

export const obsAvailableDevices = writable<ObsAvailableDevices | null>(null);
export const obsDeviceListeners = writable<DeviceListener[]>([]);
export const obsDeviceListenerStatuses = writable<DeviceListenerStatus[]>([]);

export function handleObsDevicesMessage(msg: WsMessage): void {
  if (msg.type === 'obs.devices.available') {
    obsAvailableDevices.set(msg.devices);
    obsDeviceListenerStatuses.set(msg.listenerStatuses);
  } else if (msg.type === 'obs.listeners.list') {
    obsDeviceListeners.set(msg.listeners);
    obsDeviceListenerStatuses.set(msg.statuses);
  } else if (msg.type === 'obs.listeners.create') {
    obsDeviceListeners.update((list) => [...list, msg.listener]);
  } else if (msg.type === 'obs.listeners.update') {
    obsDeviceListeners.update((list) =>
      list.map((l) => (l.id === msg.listener.id ? msg.listener : l)),
    );
  } else if (msg.type === 'obs.listeners.delete') {
    obsDeviceListeners.update((list) => list.filter((l) => l.id !== msg.id));
    obsDeviceListenerStatuses.update((list) => list.filter((s) => s.listenerId !== msg.id));
  }
}
