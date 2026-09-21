import { apiFetch } from './client.js';
import {
  TitleTemplateSchema,
  SlideFolderSchema,
  type TitleTemplate,
  type SlideFolder,
} from '../schemas/settings.js';

export function getTitleTemplate(): Promise<TitleTemplate> {
  return apiFetch('/api/settings/title-template', TitleTemplateSchema);
}

export function setTitleTemplate(template: string): Promise<TitleTemplate> {
  return apiFetch('/api/settings/title-template', TitleTemplateSchema, {
    method: 'PUT',
    body: { template },
  });
}

/** Where the core writes generated Bible slide decks. Empty means unconfigured. */
export function getSlideFolder(): Promise<SlideFolder> {
  return apiFetch('/api/settings/slide-folder', SlideFolderSchema);
}

/** Where the core writes generated song slide decks. Empty means unconfigured. */
export function getSongSlideFolder(): Promise<SlideFolder> {
  return apiFetch('/api/settings/song-slide-folder', SlideFolderSchema);
}

export function setSongSlideFolder(path: string): Promise<SlideFolder> {
  return apiFetch('/api/settings/song-slide-folder', SlideFolderSchema, {
    method: 'PUT',
    body: { path },
  });
}

export function setSlideFolder(path: string): Promise<SlideFolder> {
  return apiFetch('/api/settings/slide-folder', SlideFolderSchema, {
    method: 'PUT',
    body: { path },
  });
}
