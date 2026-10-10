import { z } from 'zod';
import { apiFetch } from './client.js';
import { sendWsCommand } from '../ws/client.js';
import { OpenAiVerifyResultSchema, type OpenAiVerifyResult } from '../schemas/transcription.js';

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
