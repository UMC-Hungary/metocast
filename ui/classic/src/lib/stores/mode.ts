import { writable } from 'svelte/store';

export type AppMode = 'server' | 'client';

export const appMode = writable<AppMode>('server');
