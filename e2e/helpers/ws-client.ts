/**
 * WebSocket client helper for E2E tests.
 *
 * Connects to ws://localhost:3737/ws?token=<TAURI_TEST_TOKEN>
 * and provides helpers to send commands and wait for typed responses.
 *
 * Timeouts are handled by vitest's testTimeout (configured in vitest.config.ts).
 */

import WebSocket from 'ws';

const WS_URL = process.env.TAURI_TEST_WS_URL ?? 'ws://localhost:3737';
const TOKEN = process.env.TAURI_TEST_TOKEN ?? '';

export type WsClientMessage = Record<string, unknown>;

type PendingListener = {
  resolve: (msg: WsClientMessage) => void;
  reject: (err: Error) => void;
  type: string;
};

export class WsTestClient {
  private ws: WebSocket;
  private messageQueue: WsClientMessage[] = [];
  private pending: PendingListener[] = [];
  private connectResolve?: () => void;
  private connectReject?: (err: Error) => void;
  private connected = false;

  constructor() {
    this.ws = new WebSocket(`${WS_URL}/ws?token=${TOKEN}`);

    this.ws.on('message', (data: WebSocket.Data) => {
      const raw = data.toString();
      let msg: WsClientMessage;
      try {
        msg = JSON.parse(raw) as WsClientMessage;
      } catch {
        return;
      }

      if (msg['type'] === 'connected') {
        this.connected = true;
        this.connectResolve?.();
        this.connectResolve = undefined;
        this.connectReject = undefined;
        return;
      }

      const idx = this.pending.findIndex((p) => p.type === msg['type']);
      if (idx !== -1) {
        const [listener] = this.pending.splice(idx, 1);
        listener.resolve(msg);
      } else {
        this.messageQueue.push(msg);
      }
    });

    this.ws.on('error', (err) => {
      const error = err instanceof Error ? err : new Error(String(err));
      if (this.connectReject) {
        this.connectReject(error);
        this.connectResolve = undefined;
        this.connectReject = undefined;
      }
      for (const p of this.pending) {
        p.reject(error);
      }
      this.pending = [];
    });
  }

  /** Wait until the WebSocket emits a 'connected' message. */
  async waitForConnect(): Promise<void> {
    if (this.connected) return;
    return new Promise<void>((resolve, reject) => {
      this.connectResolve = resolve;
      this.connectReject = reject;
    });
  }

  /** Send a WS command to the server. */
  send(command: WsClientMessage): void {
    this.ws.send(JSON.stringify(command));
  }

  /**
   * Wait for the next message whose `type` matches the given value.
   * Checks the already-received queue first, then waits for new messages.
   */
  async waitForMessage(type: string): Promise<WsClientMessage> {
    const existing = this.messageQueue.find((m) => m['type'] === type);
    if (existing) {
      this.messageQueue = this.messageQueue.filter((m) => m !== existing);
      return existing;
    }
    return new Promise<WsClientMessage>((resolve, reject) => {
      this.pending.push({ resolve, reject, type });
    });
  }

  /** Send a command and wait for a response message of the given type. */
  async command(cmd: WsClientMessage, responseType: string): Promise<WsClientMessage> {
    const pending = this.waitForMessage(responseType);
    this.send(cmd);
    return pending;
  }

  /** Close the WebSocket connection. */
  close(): void {
    this.ws.close();
  }
}
