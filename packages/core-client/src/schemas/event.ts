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

/** `schedule_status`: Metocast's own states, then the platform's, the same for every platform. */
export const BroadcastStatusSchema = z.enum([
  'not_scheduled',
  'failed',
  'scheduled',
  'starting',
  'live',
  'ended',
]);

export type BroadcastStatus = z.infer<typeof BroadcastStatusSchema>;

export type BroadcastPlatform = 'youtube' | 'facebook';

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
  connections: z.array(EventConnectionSchema).default([]),
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

// ── Order of Service ──────────────────────────────────────────────────────────

export const OrderItemKindSchema = z.enum(['song', 'bible', 'block', 'announcements', 'text']);

/**
 * One step: a song deck, one of the event's readings, the announcements (their `notes` are the
 * bullet points, one per line, shown as slides), a text slide (the title and its `notes` as
 * optional paragraphs, such as the welcome), or a block with no slides.
 */
export const OrderItemSchema = z.object({
  /** Absent for a step added in this edit; send it back to keep the step live across edits. */
  id: z.string().uuid().nullish(),
  kind: OrderItemKindSchema,
  title: z.string().default(''),
  deckPath: z.string().nullish(),
  bibleType: z.enum(['textus', 'leckio']).nullish(),
  /** A Bible step's own passage when `bibleType` is absent; the server stores its verses. */
  reference: z.string().nullish(),
  /** The translation of `reference`; RUF_v2 when absent. */
  translation: z.string().nullish(),
  notes: z.string().default(''),
});

export const EventOrderSchema = z.object({ items: z.array(OrderItemSchema) });

/** The step on screen, pushed as `order.state`. */
export const OrderPositionSchema = z.object({
  eventId: z.string().uuid(),
  itemId: z.string().uuid(),
});

export type OrderItemKind = z.infer<typeof OrderItemKindSchema>;
export type OrderItem = z.infer<typeof OrderItemSchema>;
export type EventOrder = z.infer<typeof EventOrderSchema>;
export type OrderPosition = z.infer<typeof OrderPositionSchema>;
