import { writable } from 'svelte/store';
import { PresenterStateSchema } from '@metocast/core-client/schemas/ws-messages';
import type {
  PresenterState,
  PresenterTheme,
  WsClientInfo,
} from '@metocast/core-client/schemas/ws-messages';

/** Reflects the backend-persisted use_web_presenter setting; updated on every presentation.settings push. */
export const useWebPresenter = writable<boolean>(false);
export const presenterTheme = writable<PresenterTheme>('classic');

const emptyState: PresenterState = PresenterStateSchema.parse({
  loaded: false,
  filePath: null,
  currentSlide: 0,
  totalSlides: 0,
  slides: [],
  muted: false,
});

export const presenterState = writable<PresenterState>(emptyState);

export const connectedClients = writable<WsClientInfo[]>([]);
