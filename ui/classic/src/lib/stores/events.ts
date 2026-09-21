import { writable } from 'svelte/store';
import type { EventSummary } from '@metocast/core-client/schemas/event';
import type { UntrackedRecording } from '@metocast/core-client/schemas/untracked-recording';

export const events = writable<EventSummary[]>([]);
export const eventsLoading = writable<boolean>(false);
export const untrackedRecordings = writable<UntrackedRecording[]>([]);
