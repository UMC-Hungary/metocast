import { z } from 'zod';

export const RecordingUploadSchema = z.object({
  recordingId: z.string().uuid(),
  platform: z.string(),
  state: z.enum(['pending', 'uploading', 'paused', 'completed', 'failed']),
  progressBytes: z.number().int(),
  totalBytes: z.number().int(),
  visibility: z.string(),
  videoId: z.string().nullish(),
  videoUrl: z.string().nullish(),
  error: z.string().nullish(),
  startedAt: z.string().nullish(),
  completedAt: z.string().nullish(),
  updatedAt: z.string(),
});

export type RecordingUpload = z.infer<typeof RecordingUploadSchema>;

export const RecordingSchema = z.object({
  id: z.string().uuid(),
  eventId: z.string().uuid(),
  filePath: z.string(),
  fileName: z.string(),
  fileSize: z.number().int().nonnegative(),
  durationSeconds: z.number().nonnegative(),
  mediaKind: z.string(),
  source: z.string(),
  metadata: z.record(z.string(), z.unknown()),
  detectedAt: z.string(),
  whitelisted: z.boolean(),
  uploaded: z.boolean(),
  uploadedAt: z.string().nullish(),
  videoId: z.string().nullish(),
  videoUrl: z.string().nullish(),
  customTitle: z.string().nullish(),
  uploadable: z.boolean().default(false),
  customDescription: z.string().nullish(),
  uploads: z.array(RecordingUploadSchema).default([]),
  createdAt: z.string(),
  updatedAt: z.string(),
});

export type Recording = z.infer<typeof RecordingSchema>;

export const CreateRecordingPayloadSchema = z.object({
  file_path: z.string().min(1),
  file_name: z.string().min(1),
  file_size: z.number().int().nonnegative().optional(),
  duration_seconds: z.number().nonnegative().optional(),
  custom_title: z.string().optional(),
  custom_description: z.string().optional(),
});

export type CreateRecordingPayload = z.infer<typeof CreateRecordingPayloadSchema>;

export const FlagUploadItemSchema = z.object({
  recording_id: z.string().uuid(),
  custom_title: z.string().optional(),
  custom_description: z.string().optional(),
  youtube_visibility: z.enum(['private', 'unlisted', 'public']).optional(),
  facebook_visibility: z.enum(['ONLY_ME', 'FRIENDS', 'EVERYONE']).optional(),
  platforms: z.array(z.string()),
});

export type FlagUploadItem = z.infer<typeof FlagUploadItemSchema>;

export const FlagUploadRequestSchema = z.object({
  recordings: z.array(FlagUploadItemSchema),
});

export type FlagUploadRequest = z.infer<typeof FlagUploadRequestSchema>;

export const RecordingWithEventSchema = RecordingSchema.extend({
  eventTitle: z.string(),
});
export type RecordingWithEvent = z.infer<typeof RecordingWithEventSchema>;
