import { writable } from 'svelte/store';
import type { KeynoteStatus, PptFile } from '@metocast/core-client/schemas/ws-messages';

export type { KeynoteStatus, PptFile };

export interface PptFolder {
  id: string;
  path: string;
  name: string;
  sortOrder: number;
}

export const keynoteStatus = writable<KeynoteStatus>({
  appRunning: false,
  slideshowActive: false,
  currentSlide: null,
  totalSlides: null,
  documentName: null,
});

export const pptFilter = writable<string>('');
export const pptResults = writable<PptFile[]>([]);
export const pptFolders = writable<PptFolder[]>([]);
