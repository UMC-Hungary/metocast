import { z } from 'zod';

/**
 * Media processing: the records of `/api/media` and the `media.render.progress`,
 * `media.audio.progress` and `media.program.progress` messages.
 */

export const CropRectSchema = z.object({
  x: z.number().int().nonnegative(),
  y: z.number().int().nonnegative(),
  width: z.number().int().positive(),
  height: z.number().int().positive(),
});

export const AudioInfoSchema = z.object({
  codec: z.string(),
  channels: z.number().int(),
  sampleRate: z.number().int(),
  bits: z.number().int().nullable(),
});

/** One render's settings; `null` ISO, white balance, tint and exposure keep what was shot. */
export const RenderSettingsSchema = z.object({
  gamma: z.string(),
  gamut: z.string(),
  iso: z.number().int().nullable(),
  kelvin: z.number().int().nullable(),
  tint: z.number().int().nullable(),
  exposure: z.number().nullable(),
  highlightRecovery: z.boolean(),
  gamutCompression: z.boolean(),
  /** `sidecar`: the LUT inside `sidecarPath` (or the clip's own .sidecar). */
  lutMode: z.enum(['embedded', 'file', 'sidecar', 'none']),
  lutPath: z.string().nullable(),
  aspect: z.enum(['16:9', 'native']),
  cropShiftY: z.number().int(),
  bitrateMbps: z.number().int().min(10).max(400),
  audioChannels: z.array(z.number().int().min(1)).max(2),
  /** A Blackmagic RAW .sidecar (such as Resolve's) applied before the other settings. */
  sidecarPath: z.string().nullable(),
});

export const ValueRangeSchema = z.object({ min: z.number(), max: z.number() });

/** What the Blackmagic RAW SDK lets a clip's settings be, and what it was shot with. */
export const RawOptionsSchema = z.object({
  gammas: z.array(z.string()),
  gamuts: z.array(z.string()),
  isos: z.array(z.number().int()),
  kelvin: ValueRangeSchema.nullable(),
  tint: ValueRangeSchema.nullable(),
  exposure: ValueRangeSchema.nullable(),
  asShot: z.object({
    iso: z.number().int().nullable(),
    kelvin: z.number().int().nullable(),
    tint: z.number().int().nullable(),
    exposure: z.number().nullable(),
  }),
});

/** One frame as a base64 JPEG, and the crop a render keeps, in source pixels. */
export const PreviewFrameSchema = z.object({
  imageJpeg: z.string(),
  active: CropRectSchema,
  output: CropRectSchema,
});

/** A transcript to move onto a program's voice and send with its uploads. */
export const SubtitleSourceSchema = z.object({
  path: z.string(),
  language: z.string().regex(/^[a-z]{2,3}$/),
});

export const SubtitleReportSchema = z.object({
  language: z.string(),
  cues: z.number().int(),
  /** How far the transcript was moved to meet the voice. */
  shiftSeconds: z.number(),
  /** `false`: the voice didn't back a shift clearly, so it was left unmoved. */
  confident: z.boolean(),
  srtPath: z.string(),
});

export const MediaProbeSchema = z.object({
  path: z.string(),
  fileName: z.string(),
  fileSize: z.number().int(),
  kind: z.enum(['braw', 'video']),
  camera: z.string().nullable(),
  clipName: z.string().nullable(),
  width: z.number().int(),
  height: z.number().int(),
  activeArea: CropRectSchema,
  fpsNum: z.number().int(),
  fpsDen: z.number().int(),
  frameCount: z.number().int(),
  durationSeconds: z.number(),
  startTimecode: z.string().nullable(),
  /** By the camera's clock, which may be wrong. */
  recordedAt: z.string().nullable(),
  /** When Metocast saw the camera start the clip, by the server's clock. */
  hostRecordStartedAt: z.string().nullable(),
  audio: AudioInfoSchema.nullable(),
  gamma: z.string().nullable(),
  gamut: z.string().nullable(),
  embeddedLut: z.string().nullable(),
  defaults: RenderSettingsSchema,
  output: CropRectSchema,
  missing: z.array(z.string()),
  /** Blackmagic RAW only. */
  rawOptions: RawOptionsSchema.nullable(),
});

export const RenderQueuedSchema = z.object({
  jobId: z.string().uuid(),
  outputPath: z.string(),
});

export const RenderProgressSchema = z.object({
  jobId: z.string().uuid(),
  state: z.enum(['running', 'succeeded', 'failed', 'cancelled']),
  frame: z.number().int(),
  totalFrames: z.number().int(),
  fps: z.number(),
  outputPath: z.string(),
  error: z.string().nullable(),
});

/** One audio job's settings. Empty `channels` mixes them all. */
export const AudioSettingsSchema = z.object({
  channels: z.array(z.number().int().min(1).max(64)),
  targetLufs: z.number().min(-30).max(-9),
});

export const AudioSourceSchema = z.object({
  path: z.string(),
  fileName: z.string(),
  fileSize: z.number().int(),
  audio: AudioInfoSchema,
  durationSeconds: z.number(),
  /** The start of the audio by the recorder's clock. */
  recordedAt: z.string().nullable(),
});

export const AudioProbeSchema = z.object({
  sources: z.array(AudioSourceSchema),
  durationSeconds: z.number(),
  channels: z.number().int(),
  recordedAt: z.string().nullable(),
  defaults: AudioSettingsSchema,
  missing: z.array(z.string()),
  /** Tools the first job downloads, such as `jive-vocals`. */
  downloads: z.array(z.string()),
});

export const AudioReportSchema = z.object({
  inputLufs: z.number(),
  outputLufs: z.number(),
  outputTruePeakDbtp: z.number(),
  /** The delay jive-vocals added, in 48 kHz samples, which Metocast removed. */
  removedDelaySamples: z.number().int(),
  alignmentConfidence: z.number(),
});

export const AudioProgressSchema = z.object({
  jobId: z.string().uuid(),
  state: z.enum(['running', 'succeeded', 'failed', 'cancelled']),
  step: z.enum(['download', 'prepare', 'clean', 'align', 'finish']).nullable(),
  fraction: z.number().nullable(),
  outputPath: z.string(),
  error: z.string().nullable(),
  report: AudioReportSchema.nullable(),
});

/** The video's own sound, measured at a few points. */
export const VideoSoundSchema = z.object({
  hasTrack: z.boolean(),
  /** Loud enough to sync by; the Cinema Camera's unconnected input reads −97 dBFS. */
  usable: z.boolean(),
  levelDbfs: z.number().nullable(),
});

/** Where the clocks put the picture's start in the joined audio. */
export const ClockEstimateSchema = z.object({
  offsetSeconds: z.number(),
  /** Corrected by a clock difference learned from a recent sync of the same camera. */
  learned: z.boolean(),
});

export const SyncMethodSchema = z.enum(['none', 'waveform', 'lips', 'clock', 'manual']);
export const PictureModeSchema = z.enum(['grade', 'copy']);

/** A video and its audio, and how a program job would put them together. */
export const ProgramProbeSchema = z.object({
  video: MediaProbeSchema,
  /** `null` when the video's own sound is used. */
  audio: AudioProbeSchema.nullable(),
  videoSound: VideoSoundSchema,
  syncMethod: SyncMethodSchema,
  clock: ClockEstimateSchema.nullable(),
  picture: PictureModeSchema,
  warnings: z.array(z.string()),
  /** Why the job can't run, when it can't. */
  blocker: z.string().nullable(),
  missing: z.array(z.string()),
  downloads: z.array(z.string()),
});

export const SyncWindowSchema = z.object({
  videoSeconds: z.number(),
  offsetSeconds: z.number(),
  score: z.number(),
  agrees: z.boolean(),
});

/** How a program's audio was lined up with its picture. */
export const SyncReportSchema = z.object({
  method: SyncMethodSchema,
  /** Where the picture's first frame falls in the joined audio; negative when it starts later. */
  offsetSeconds: z.number(),
  driftPpm: z.number(),
  confidence: z.number(),
  /** `false` asks the operator to check the sync. */
  confident: z.boolean(),
  peakSigma: z.number().nullable(),
  windows: z.array(SyncWindowSchema),
  clockDifferenceSeconds: z.number().nullable(),
  clockOffsetSeconds: z.number().nullable(),
  /** Frames moved by hand after the job; positive plays the audio later. */
  manualFrames: z.number().int(),
  startFile: z.string().nullable(),
  startFileSeconds: z.number().nullable(),
});

export const ProgramReportSchema = z.object({
  sync: SyncReportSchema,
  audio: AudioReportSchema,
  audioSeconds: z.number(),
  /** The share of the picture the audio covers (0–1). */
  coverage: z.number(),
  silentStartSeconds: z.number(),
  silentEndSeconds: z.number(),
  /** Frames left off the picture's end because the clip's last seconds couldn't be read. */
  damagedFrames: z.number().int().nonnegative(),
  audioOutputPath: z.string(),
  subtitles: z.array(SubtitleReportSchema),
});

export const ProgramProgressSchema = z.object({
  /** The program job, also while it's being nudged. */
  jobId: z.string().uuid(),
  state: z.enum(['running', 'succeeded', 'failed', 'cancelled']),
  step: z.enum(['download', 'picture', 'prepare', 'sync', 'clean', 'merge']).nullable(),
  fraction: z.number().nullable(),
  outputPath: z.string(),
  error: z.string().nullable(),
  report: ProgramReportSchema.nullable(),
});

export type CropRect = z.infer<typeof CropRectSchema>;
export type AudioInfo = z.infer<typeof AudioInfoSchema>;
export type RenderSettings = z.infer<typeof RenderSettingsSchema>;
export type MediaProbe = z.infer<typeof MediaProbeSchema>;
export type RawOptions = z.infer<typeof RawOptionsSchema>;
export type PreviewFrame = z.infer<typeof PreviewFrameSchema>;
export type SubtitleSource = z.infer<typeof SubtitleSourceSchema>;
export type SubtitleReport = z.infer<typeof SubtitleReportSchema>;
export type RenderQueued = z.infer<typeof RenderQueuedSchema>;
export type RenderProgress = z.infer<typeof RenderProgressSchema>;
export type AudioSettings = z.infer<typeof AudioSettingsSchema>;
export type AudioSource = z.infer<typeof AudioSourceSchema>;
export type AudioProbe = z.infer<typeof AudioProbeSchema>;
export type AudioReport = z.infer<typeof AudioReportSchema>;
export type AudioProgress = z.infer<typeof AudioProgressSchema>;
export type VideoSound = z.infer<typeof VideoSoundSchema>;
export type ClockEstimate = z.infer<typeof ClockEstimateSchema>;
export type SyncMethod = z.infer<typeof SyncMethodSchema>;
export type PictureMode = z.infer<typeof PictureModeSchema>;
export type ProgramProbe = z.infer<typeof ProgramProbeSchema>;
export type SyncWindow = z.infer<typeof SyncWindowSchema>;
export type SyncReport = z.infer<typeof SyncReportSchema>;
export type ProgramReport = z.infer<typeof ProgramReportSchema>;
export type ProgramProgress = z.infer<typeof ProgramProgressSchema>;
