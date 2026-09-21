import { writable } from 'svelte/store';

export type UploadProgressEntry = {
  platform: string;
  progressBytes: number;
  totalBytes: number;
  state: 'uploading' | 'completed' | 'failed' | 'paused';
  videoId?: string;
  videoUrl?: string;
  error?: string;
};

// Key: `${recordingId}:${platform}`
export const uploadProgress = writable<Record<string, UploadProgressEntry>>({});
