import { z } from 'zod';

export const BibleVerseSchema = z.object({
  chapter: z.number(),
  verse: z.number(),
  text: z.string(),
});

export type BibleVerse = z.infer<typeof BibleVerseSchema>;

export const BibleReferenceSchema = z.object({
  type: z.string(),
  reference: z.string(),
  translation: z.string(),
  verses: z.array(BibleVerseSchema).default([]),
});

export type BibleReference = z.infer<typeof BibleReferenceSchema>;

export const EventConnectionSchema = z.object({
  platform: z.string(),
  externalId: z.string().nullable().default(null),
  streamUrl: z.string().nullable().default(null),
  eventUrl: z.string().nullable().default(null),
  scheduleStatus: z.string().default('not_scheduled'),
  privacyStatus: z.string().nullable().default(null),
  extra: z
    .record(z.string(), z.union([z.string(), z.number(), z.boolean(), z.null()]))
    .nullable()
    .default(null),
});

export type EventConnection = z.infer<typeof EventConnectionSchema>;

export const EventSchema = z.object({
  id: z.string().uuid(),
  title: z.string(),
  computedTitle: z.string().default(''),
  dateTime: z.string(),
  speaker: z.string(),
  description: z.string(),
  autoUploadEnabled: z.boolean(),
  connections: z.array(EventConnectionSchema).default([]),
  bibleReferences: z.array(BibleReferenceSchema).default([]),
  createdAt: z.string(),
  updatedAt: z.string(),
});

export type Event = z.infer<typeof EventSchema>;

export const EventSummarySchema = z.object({
  id: z.string().uuid(),
  title: z.string(),
  computedTitle: z.string().default(''),
  dateTime: z.string(),
  speaker: z.string(),
  recordingCount: z.number().int().nonnegative(),
  isCompleted: z.boolean(),
  createdAt: z.string(),
  updatedAt: z.string(),
});

export type EventSummary = z.infer<typeof EventSummarySchema>;

export const EventActivitySchema = z.object({
  id: z.string().uuid(),
  eventId: z.string().uuid(),
  activityType: z.string(),
  message: z.string().nullable(),
  createdAt: z.string(),
});

export type EventActivity = z.infer<typeof EventActivitySchema>;

export type CreateEventActivityPayload = {
  activity_type: string;
  message?: string;
};

const BibleReferencePayloadSchema = z.object({
  type: z.string(),
  reference: z.string().optional(),
  translation: z.string().optional(),
  verses: z
    .array(z.object({ chapter: z.number(), verse: z.number(), text: z.string() }))
    .optional(),
});

export const CreateEventPayloadSchema = z.object({
  title: z.string().min(1),
  computed_title: z.string().optional(),
  date_time: z.string(),
  speaker: z.string().optional(),
  description: z.string().optional(),
  auto_upload_enabled: z.boolean().optional(),
  connections: z
    .array(z.object({ platform: z.string(), privacy_status: z.string().optional() }))
    .optional(),
  bible_references: z.array(BibleReferencePayloadSchema).optional(),
});

export type CreateEventPayload = z.infer<typeof CreateEventPayloadSchema>;

export const UpdateEventPayloadSchema = CreateEventPayloadSchema;
export type UpdateEventPayload = CreateEventPayload;
