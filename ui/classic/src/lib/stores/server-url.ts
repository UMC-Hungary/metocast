import { writable } from 'svelte/store';

export const serverUrl = writable<string>('http://localhost:3737');
export const serverPort = writable<number>(3737);
export const authToken = writable<string>('');
export const localNetworkUrl = writable<string>('');
/** Set to true once ConnectorInit has resolved the app mode and server URL. */
export const appReady = writable<boolean>(false);
