import { z } from 'zod';
import { apiFetch } from './client.js';

export function triggerUploadCycle(): Promise<void> {
  return apiFetch('/api/uploads/trigger', z.void(), { method: 'POST' });
}
