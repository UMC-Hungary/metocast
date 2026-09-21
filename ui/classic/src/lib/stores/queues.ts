import { writable } from 'svelte/store';
import type { QueueSummary } from '@metocast/core-client/schemas/queue';

/// Live depth-by-status per queue, pushed by the `queue.stats` WS message.
export const queues = writable<QueueSummary[]>([]);
