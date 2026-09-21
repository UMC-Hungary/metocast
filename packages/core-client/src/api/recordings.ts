import { z } from 'zod';
import { apiFetch } from './client.js';
import {
  RecordingSchema,
  RecordingWithEventSchema,
  type Recording,
  type RecordingWithEvent,
  type CreateRecordingPayload,
  type FlagUploadItem,
} from '../schemas/recording.js';

export function deleteRecording(
  eventId: string,
  recordingId: string,
  deleteFile = false,
): Promise<void> {
  const qs = deleteFile ? '?delete_file=true' : '';
  return apiFetch(`/api/events/${eventId}/recordings/${recordingId}${qs}`, z.void(), {
    method: 'DELETE',
  });
}

export function listRecordings(eventId: string): Promise<Recording[]> {
  return apiFetch(`/api/events/${eventId}/recordings`, z.array(RecordingSchema));
}

export function createRecording(
  eventId: string,
  payload: CreateRecordingPayload,
): Promise<Recording> {
  return apiFetch(`/api/events/${eventId}/recordings`, RecordingSchema, {
    method: 'POST',
    body: payload,
  });
}

export function flagRecordingsForUpload(eventId: string, items: FlagUploadItem[]): Promise<void> {
  return apiFetch(`/api/events/${eventId}/recordings/flag-upload`, z.void(), {
    method: 'POST',
    body: { recordings: items },
  });
}

export function listAllRecordings(
  filter?: 'not_flagged' | 'flagged' | 'in_progress' | 'uploaded',
): Promise<RecordingWithEvent[]> {
  const qs = filter ? `?filter=${filter}` : '';
  return apiFetch(`/api/recordings${qs}`, z.array(RecordingWithEventSchema));
}
