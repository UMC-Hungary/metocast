import { z } from 'zod';

/** Only the platform_sync job types exist today; both key off one of these. */
export const JobPayloadSchema = z.object({
  event_id: z.string().uuid().optional(),
  broadcast_id: z.string().optional(),
});

export const JobSchema = z.object({
  id: z.string().uuid(),
  queue: z.string(),
  jobType: z.string(),
  dedupKey: z.string().nullable(),
  payload: JobPayloadSchema,
  status: z.string(),
  attempts: z.number().int(),
  maxAttempts: z.number().int(),
  availableAt: z.string(),
  lockedAt: z.string().nullable(),
  lockedBy: z.string().nullable(),
  lastError: z.string().nullable(),
  createdAt: z.string(),
  updatedAt: z.string(),
});

export const QueueSummarySchema = z.object({
  queue: z.string(),
  pending: z.number().int(),
  processing: z.number().int(),
  succeeded: z.number().int(),
  dead: z.number().int(),
  oldestAvailableAt: z.string().nullable(),
});

export type Job = z.infer<typeof JobSchema>;
export type QueueSummary = z.infer<typeof QueueSummarySchema>;
