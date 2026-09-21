import { z } from 'zod';
import { apiFetch } from './client.js';

export const CronJobSchema = z.object({
  id: z.string(),
  name: z.string(),
  cronExpression: z.string(),
  enabled: z.boolean(),
  pullYoutube: z.boolean(),
  autoUpload: z.boolean(),
  createdAt: z.string(),
  updatedAt: z.string(),
});

export type CronJob = z.infer<typeof CronJobSchema>;

const CronJobArraySchema = z.array(CronJobSchema);

export function listCronJobs(): Promise<CronJob[]> {
  return apiFetch('/api/cron-jobs', CronJobArraySchema);
}

export function createCronJob(body: {
  name: string;
  cronExpression: string;
  enabled: boolean;
  pullYoutube: boolean;
  autoUpload: boolean;
}): Promise<CronJob> {
  return apiFetch('/api/cron-jobs', CronJobSchema, { method: 'POST', body });
}

export function updateCronJob(
  id: string,
  body: {
    name: string;
    cronExpression: string;
    enabled: boolean;
    pullYoutube: boolean;
    autoUpload: boolean;
  },
): Promise<CronJob> {
  return apiFetch(`/api/cron-jobs/${id}`, CronJobSchema, { method: 'PUT', body });
}

export function deleteCronJob(id: string): Promise<void> {
  return apiFetch(`/api/cron-jobs/${id}`, z.void(), { method: 'DELETE' });
}
