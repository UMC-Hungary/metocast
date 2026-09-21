import { z } from 'zod';
import { apiFetch } from './client.js';
import { ApplicationLogSchema, type ApplicationLog } from '../schemas/logs.js';

export type { ApplicationLog };

/** The core's own log — the server's when this UI is a client of a remote core. */
export function fetchApplicationLog(): Promise<ApplicationLog> {
  return apiFetch('/api/logs', ApplicationLogSchema);
}

export function clearApplicationLog(): Promise<void> {
  return apiFetch('/api/logs', z.void(), { method: 'DELETE' });
}
