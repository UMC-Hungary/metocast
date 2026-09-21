import { z } from 'zod';

export const UntrackedRecordingSchema = z.object({
  id: z.string().uuid(),
  filePath: z.string(),
  fileName: z.string(),
  fileSize: z.number().int().nonnegative(),
  durationSeconds: z.number().nonnegative(),
  detectedAt: z.string(),
  createdAt: z.string(),
});

export type UntrackedRecording = z.infer<typeof UntrackedRecordingSchema>;
