import { derived, writable } from 'svelte/store';

export const serverUrl = writable<string>('http://localhost:3737');
export const serverPort = writable<number>(3737);
export const authToken = writable<string>('');
/** Addresses other devices reach this server at, best first: LAN IP, then `.local`. */
export const localNetworkUrls = writable<string[]>([]);
/** The address every device can reach (the LAN IP), or '' when unknown. */
export const localNetworkUrl = derived(localNetworkUrls, (urls) => urls[0] ?? '');
/** Set to true once ConnectorInit has resolved the app mode and server URL. */
export const appReady = writable<boolean>(false);
