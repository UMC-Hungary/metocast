import { z } from 'zod';
import { apiFetch } from './client.js';
import {
  UntrackedRecordingSchema,
  type UntrackedRecording,
} from '../schemas/untracked-recording.js';
import { RecordingSchema, type Recording } from '../schemas/recording.js';

export function listUntrackedRecordings(): Promise<UntrackedRecording[]> {
  return apiFetch('/api/recordings/untracked', z.array(UntrackedRecordingSchema));
}

export function assignUntrackedRecording(id: string, eventId: string): Promise<Recording> {
  return apiFetch(`/api/recordings/untracked/${id}/assign`, RecordingSchema, {
    method: 'POST',
    body: { event_id: eventId },
  });
}

export function deleteUntrackedRecording(id: string, deleteFile: boolean): Promise<void> {
  return apiFetch(`/api/recordings/untracked/${id}?delete_file=${deleteFile}`, z.undefined(), {
    method: 'DELETE',
  });
}
