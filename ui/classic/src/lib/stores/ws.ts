import { writable } from 'svelte/store';
import type { WsMessage } from '@metocast/core-client/schemas/ws-messages';

export type WsStatus = 'connecting' | 'connected' | 'disconnected' | 'error';

export const wsStatus = writable<WsStatus>('disconnected');
export const lastWsMessage = writable<WsMessage | null>(null);
