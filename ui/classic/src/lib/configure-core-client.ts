/**
 * Wires this UI's config stores into the shared client, once, at startup. Imported
 * for side effect by the root layout so every api/ws call reads live values.
 */
import { get } from 'svelte/store';
import { serverUrl, serverPort, authToken } from '$lib/stores/server-url.js';
import { appMode } from '$lib/stores/mode.js';
import { configureCoreClient } from '@metocast/core-client';

configureCoreClient(() => ({
  serverUrl: get(serverUrl),
  serverPort: get(serverPort),
  authToken: get(authToken),
  mode: get(appMode),
}));
