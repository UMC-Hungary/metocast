import { WsMessageSchema } from '../schemas/ws-messages.js';
import type { WsMessage } from '../schemas/ws-messages.js';
import { coreConfig } from '../config.js';
import { TranscriptMessageSchema } from '../schemas/transcription.js';
import type { TranscriptMessage } from '../schemas/transcription.js';

export type WsStatus = 'connecting' | 'connected' | 'disconnected' | 'error';

/**
 * A UI binds the transport to its own state. The transport owns the socket,
 * reconnection and schema validation; the handlers own everything UI-shaped —
 * which stores to write, which notifications to raise, what to send on connect.
 */
export interface WsHandlers {
  onStatus: (status: WsStatus) => void;
  onMessage: (msg: WsMessage) => void;
  onOpen?: () => void;
}

let socket: WebSocket | null = null;
let handlers: WsHandlers | null = null;
let reconnectTimer: ReturnType<typeof setTimeout> | null = null;
let reconnectDelay = 3000;
const RECONNECT_DELAY_MIN = 3000;
const RECONNECT_DELAY_MAX = 30000;

export function connectWs(h: WsHandlers): void {
  handlers = h;
  if (
    socket &&
    (socket.readyState === WebSocket.OPEN || socket.readyState === WebSocket.CONNECTING)
  )
    return;

  const { mode, authToken: token, serverPort, serverUrl } = coreConfig();

  let wsUrl: string;
  if (mode === 'server') {
    wsUrl = `ws://localhost:${serverPort}/ws?token=${encodeURIComponent(token)}`;
  } else {
    const base = serverUrl.replace(/^http/, 'ws');
    wsUrl = `${base}/ws?token=${encodeURIComponent(token)}`;
  }

  handlers.onStatus('connecting');
  socket = new WebSocket(wsUrl);

  socket.addEventListener('open', () => {
    handlers?.onStatus('connected');
    reconnectDelay = RECONNECT_DELAY_MIN;
    if (reconnectTimer !== null) {
      clearTimeout(reconnectTimer);
      reconnectTimer = null;
    }
    handlers?.onOpen?.();
  });

  socket.addEventListener('message', (ev) => {
    const result = WsMessageSchema.safeParse(JSON.parse(ev.data as string));
    if (!result.success) return;
    handlers?.onMessage(result.data);
  });

  socket.addEventListener('close', () => {
    handlers?.onStatus('disconnected');
    socket = null;
    scheduleReconnect();
  });

  socket.addEventListener('error', () => {
    handlers?.onStatus('error');
    socket?.close();
  });
}

export function disconnectWs(): void {
  if (reconnectTimer !== null) {
    clearTimeout(reconnectTimer);
    reconnectTimer = null;
  }
  socket?.close();
  socket = null;
  reconnectDelay = RECONNECT_DELAY_MIN;
  handlers?.onStatus('disconnected');
}

function scheduleReconnect(): void {
  reconnectTimer = setTimeout(() => {
    if (handlers) connectWs(handlers);
  }, reconnectDelay);
  reconnectDelay = Math.min(reconnectDelay * 2, RECONNECT_DELAY_MAX);
}

export function sendWsCommand(
  type: string,
  data?: Record<string, string | number | boolean | null | string[]>,
): boolean {
  if (!socket || socket.readyState !== WebSocket.OPEN) return false;
  socket.send(JSON.stringify({ type, ...data }));
  return true;
}

/**
 * Opens a read-only presenter socket against the core that served this page.
 * Used by the standalone presenter view, which runs outside the app shell and so
 * has no configured server URL to reuse.
 */
export function connectPresenterWs(options: {
  token?: string | null;
  wsPort?: string | null;
  onMessage: (raw: unknown) => void;
}): WebSocket {
  const host = options.wsPort
    ? `${window.location.hostname}:${options.wsPort}`
    : window.location.host;
  const url = options.token
    ? `ws://${host}/ws?token=${encodeURIComponent(options.token)}`
    : `ws://${host}/ws`;

  const socket = new WebSocket(url);
  socket.addEventListener('message', (ev) => {
    try {
      options.onMessage(JSON.parse(ev.data as string));
    } catch {
      // Ignore frames that are not JSON.
    }
  });
  return socket;
}

/**
 * Follows the live transcript from the core that served this page, read-only and without a
 * token, reconnecting until the returned function is called. `wsPort` reaches the core when the
 * page comes from another port (the Vite dev server).
 */
export function connectTranscriptWs(options: {
  wsPort?: string | null;
  onMessage: (msg: TranscriptMessage) => void;
  onConnection?: (open: boolean) => void;
}): () => void {
  let closed = false;
  let current: WebSocket | null = null;
  const open = () => {
    const host = options.wsPort
      ? `${window.location.hostname}:${options.wsPort}`
      : window.location.host;
    const scheme = window.location.protocol === 'https:' ? 'wss' : 'ws';
    current = new WebSocket(`${scheme}://${host}/ws`);
    current.addEventListener('open', () => options.onConnection?.(true));
    current.addEventListener('message', (ev) => {
      let raw: unknown;
      try {
        raw = JSON.parse(ev.data as string);
      } catch {
        return;
      }
      const result = TranscriptMessageSchema.safeParse(raw);
      if (result.success) options.onMessage(result.data);
    });
    current.addEventListener('close', () => {
      options.onConnection?.(false);
      if (!closed) setTimeout(open, 3000);
    });
  };
  open();
  return () => {
    closed = true;
    current?.close();
  };
}
