export type AppMode = 'server' | 'client';

export interface CoreClientConfig {
  serverUrl: string;
  serverPort: number;
  authToken: string;
  mode: AppMode;
}

let read: (() => CoreClientConfig) | null = null;

/**
 * A UI supplies the core's location and token once at startup. The package
 * needs the values, not the store they happen to live in — this is what keeps
 * the HTTP and WebSocket layers free of any framework state primitive.
 */
export function configureCoreClient(reader: () => CoreClientConfig): void {
  read = reader;
}

export function coreConfig(): CoreClientConfig {
  if (!read) {
    throw new Error('core-client is not configured — call configureCoreClient() at startup');
  }
  return read();
}
