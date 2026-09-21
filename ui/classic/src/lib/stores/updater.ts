import { writable } from 'svelte/store';

export type UpdateStatus =
  | 'idle'
  | 'checking'
  | 'available'
  | 'up-to-date'
  | 'installing'
  | 'installed'
  | 'error';

export type UpdateInfo = {
  currentVersion: string;
  latestVersion: string;
  releaseUrl: string;
  releaseNotes: string;
};

type UpdaterState = {
  status: UpdateStatus;
  info: UpdateInfo | null;
  error: string | null;
  lastChecked: Date | null;
};

export const updaterStore = writable<UpdaterState>({
  status: 'idle',
  info: null,
  error: null,
  lastChecked: null,
});
