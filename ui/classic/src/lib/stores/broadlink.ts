import { writable } from 'svelte/store';

export interface BroadlinkDiscoveredDevice {
  name: string;
  host: string;
  mac: string;
  deviceType: string;
  model: string | null;
}

export interface BroadlinkLearnResult {
  code: string | null;
  error: string | null;
}

/** Devices pushed via WS during a discover scan. Reset before each scan. */
export const broadlinkDiscoveredDevices = writable<BroadlinkDiscoveredDevice[]>([]);

/** Latest learn result pushed via WS. Null when idle. */
export const broadlinkLearnResult = writable<BroadlinkLearnResult | null>(null);
