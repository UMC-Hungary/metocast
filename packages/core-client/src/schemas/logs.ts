import { z } from 'zod';

export const ApplicationLogTextSchema = z.string();
export const ApplicationLogPathSchema = z.string();

export const ApplicationLogSchema = z.object({
  path: ApplicationLogPathSchema,
  content: ApplicationLogTextSchema,
});

export type ApplicationLog = z.infer<typeof ApplicationLogSchema>;
