import {
  configureCoreClient,
  getAppMode,
  getServerPort,
  getToken,
  getClientUrl,
  getClientToken,
  getLocalHost,
} from '@metocast/core-client';
import type { CoreClientConfig } from '@metocast/core-client';

// Sanctum has no config stores; it caches the resolved values and hands the
// package a sync reader over them, the same contract classic satisfies via stores.
const cfg: CoreClientConfig = {
  serverUrl: 'http://localhost:3737',
  serverPort: 3737,
  authToken: '',
  mode: 'server',
};

configureCoreClient(() => cfg);

/**
 * Whether this window holds a token for the core. Everything that reads the core's
 * own machine — the connect code, the version, the log — stays hidden without one.
 * Safe to read during render: initCore() resolves before the app phase starts.
 */
export const hasToken = (): boolean => cfg.authToken.length > 0;

/** Which core this window drives — 'server' means the local machine owns the files. */
export const appMode = (): CoreClientConfig['mode'] => cfg.mode;

/** Resolve the core's location and token from the desktop host, once, at startup. */
export async function initCore(): Promise<void> {
  const mode = await getAppMode();
  if (mode === 'client') {
    cfg.mode = 'client';
    cfg.serverUrl = (await getClientUrl()) ?? cfg.serverUrl;
    cfg.authToken = await getClientToken();
  } else {
    cfg.mode = 'server';
    cfg.serverPort = await getServerPort();
    cfg.serverUrl = `http://localhost:${cfg.serverPort}`;
    cfg.authToken = await getToken();
  }
}

/**
 * The address a presenter opens to join. In server mode it must be the LAN address,
 * not localhost, so another device can reach it — the same URL classic hands out.
 */
export async function presenterUrl(): Promise<string> {
  const token = encodeURIComponent(cfg.authToken);
  if (cfg.mode === 'client') return `${cfg.serverUrl}/presenter?token=${token}`;
  let base = cfg.serverUrl;
  try {
    const host = await getLocalHost();
    if (host) base = `http://${host}:${cfg.serverPort}`;
  } catch {
    /* no host — fall back to the configured URL */
  }
  return `${base}/presenter?token=${token}`;
}
