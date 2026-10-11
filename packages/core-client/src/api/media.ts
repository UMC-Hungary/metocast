import { z } from 'zod';
import { apiFetch } from './client.js';
import {
  AudioProbeSchema,
  MediaProbeSchema,
  PreviewFrameSchema,
  ProgramProbeSchema,
  RenderQueuedSchema,
  type AudioProbe,
  type AudioSettings,
  type MediaProbe,
  type PictureMode,
  type PreviewFrame,
  type ProgramProbe,
  type RenderQueued,
  type RenderSettings,
  type SubtitleSource,
} from '../schemas/media.js';

/**
 * Describes a video file on the server's disk and the render the stored settings would make. With
 * `sidecarPath` (such as Resolve's .sidecar), its settings become the probe's `defaults`.
 */
export function probeMedia(path: string, sidecarPath?: string): Promise<MediaProbe> {
  return apiFetch('/api/media/probe', MediaProbeSchema, {
    method: 'POST',
    body: { path, sidecarPath: sidecarPath ?? null },
  });
}

/** One frame as a JPEG: as recorded without `settings`, else graded with them. */
export function previewMedia(
  path: string,
  frame: number,
  options: { settings?: RenderSettings; maxWidth?: number } = {},
): Promise<PreviewFrame> {
  return apiFetch('/api/media/preview', PreviewFrameSchema, {
    method: 'POST',
    body: { path, frame, settings: options.settings ?? null, maxWidth: options.maxWidth ?? null },
  });
}

/**
 * Queues a render on the server's `media` queue. `settings` replaces the stored defaults for this
 * file only; without `eventId` the finished file joins the untracked recordings.
 */
export function startRender(
  path: string,
  options: { eventId?: string; settings?: RenderSettings } = {},
): Promise<RenderQueued> {
  return apiFetch('/api/media/renders', RenderQueuedSchema, {
    method: 'POST',
    body: { path, eventId: options.eventId ?? null, settings: options.settings ?? null },
  });
}

export function cancelRender(jobId: string): Promise<void> {
  return apiFetch(`/api/media/renders/${jobId}/cancel`, z.void(), { method: 'POST' });
}

/** Describes audio files on the server's disk, joined in the order given. */
export function probeAudio(paths: string[]): Promise<AudioProbe> {
  return apiFetch('/api/media/audio/probe', AudioProbeSchema, {
    method: 'POST',
    body: { paths },
  });
}

/**
 * Queues an audio master: the files joined, cleaned by jive-vocals, kept sample-aligned with the
 * source and lifted to the target loudness. Cancel it with `cancelRender`.
 */
export function startAudio(
  paths: string[],
  options: { eventId?: string; settings?: AudioSettings } = {},
): Promise<RenderQueued> {
  return apiFetch('/api/media/audio', RenderQueuedSchema, {
    method: 'POST',
    body: { paths, eventId: options.eventId ?? null, settings: options.settings ?? null },
  });
}

/**
 * Describes a video and its audio recording (none: the video's own sound) and how a program job
 * would sync them: by the video's sound, the speaker's lips, or the clocks.
 */
export function probeProgram(videoPath: string, audioPaths: string[] = []): Promise<ProgramProbe> {
  return apiFetch('/api/media/programs/probe', ProgramProbeSchema, {
    method: 'POST',
    body: { videoPath, audioPaths },
  });
}

/**
 * Queues a program: the picture graded or copied, the audio synced to it, cleaned, and cut to
 * exactly the picture's length. Progress arrives as `media.program.progress`; cancel it with
 * `cancelRender`.
 */
export function startProgram(
  videoPath: string,
  audioPaths: string[] = [],
  options: {
    eventId?: string;
    picture?: PictureMode;
    settings?: RenderSettings;
    audioSettings?: AudioSettings;
    /** Where in the recording the video starts, when known; skips the search. */
    startSeconds?: number;
    /** Transcripts to move onto the voice and send with the uploads. */
    subtitles?: SubtitleSource[];
  } = {},
): Promise<RenderQueued> {
  return apiFetch('/api/media/programs', RenderQueuedSchema, {
    method: 'POST',
    body: {
      videoPath,
      audioPaths,
      eventId: options.eventId ?? null,
      picture: options.picture ?? null,
      settings: options.settings ?? null,
      audioSettings: options.audioSettings ?? null,
      startSeconds: options.startSeconds ?? null,
      subtitles: options.subtitles ?? [],
    },
  });
}

/** Moves a finished program's audio by whole frames; positive plays it later. */
export function nudgeProgram(jobId: string, frames: number): Promise<RenderQueued> {
  return apiFetch(`/api/media/programs/${jobId}/nudge`, RenderQueuedSchema, {
    method: 'POST',
    body: { frames },
  });
}
