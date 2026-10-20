import { z } from 'zod';
import { apiFetch, apiFetchText } from './client.js';
import { sendWsCommand } from '../ws/client.js';
import {
  CleanupProgressSchema,
  OpenAiVerifyResultSchema,
  StoredTranscriptSchema,
  TranscriptDetailSchema,
  type CleanupProgress,
  type OpenAiVerifyResult,
  type StoredTranscript,
  type TranscriptDetail,
} from '../schemas/transcription.js';

/** Checks an OpenAI API key against `GET /v1/models`; a blank key checks the stored one. */
export function verifyOpenAiKey(apiKey = ''): Promise<OpenAiVerifyResult> {
  return apiFetch('/api/openai/verify', OpenAiVerifyResultSchema, {
    method: 'POST',
    body: { apiKey },
  });
}

/** Queues English captions for a recording; progress arrives as `captions.job.state`. */
export function generateRecordingCaptions(recordingId: string): Promise<{ queued: boolean }> {
  return apiFetch(`/api/recordings/${recordingId}/captions`, z.object({ queued: z.boolean() }), {
    method: 'POST',
  });
}

/**
 * Answered by `transcription.state`, or an `error` naming why it cannot start. Without an event
 * the session belongs to none.
 */
export function startTranscription(eventId?: string): boolean {
  return sendWsCommand('transcription.start', eventId ? { eventId } : {});
}

export function stopTranscription(): boolean {
  return sendWsCommand('transcription.stop');
}

// ── Transcript library ─────────────────────────────────────────────────────

/** Stored live-transcript sessions, newest first. */
export function listTranscripts(eventId?: string): Promise<StoredTranscript[]> {
  const query = eventId ? `?eventId=${encodeURIComponent(eventId)}` : '';
  return apiFetch(`/api/transcripts${query}`, z.array(StoredTranscriptSchema));
}

export function getTranscript(id: string): Promise<TranscriptDetail> {
  return apiFetch(`/api/transcripts/${id}`, TranscriptDetailSchema);
}

/** Refused with 409 while it is live or being cleaned up. */
export function deleteTranscript(id: string): Promise<void> {
  return apiFetch(`/api/transcripts/${id}`, z.void(), { method: 'DELETE' });
}

export type TranscriptFormat = 'txt' | 'srt' | 'vtt';

/** The file's text; `clean` for the AI-cleaned lines. */
export function downloadTranscript(
  id: string,
  format: TranscriptFormat,
  clean = false,
): Promise<string> {
  return apiFetchText(`/api/transcripts/${id}/download?format=${format}&clean=${clean}`);
}

/** Starts the OpenAI clean-up; progress arrives as `transcript.cleanup.state`. */
export function cleanupTranscript(id: string): Promise<CleanupProgress> {
  return apiFetch(`/api/transcripts/${id}/cleanup`, CleanupProgressSchema, { method: 'POST' });
}
