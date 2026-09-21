import { z } from 'zod';
import { apiFetch } from './client.js';
import { JobSchema, QueueSummarySchema, type Job, type QueueSummary } from '../schemas/queue.js';

export function listQueues(): Promise<QueueSummary[]> {
  return apiFetch('/api/queues', z.array(QueueSummarySchema));
}

export function listJobs(queue: string, status?: string): Promise<Job[]> {
  const query = status ? `?status=${encodeURIComponent(status)}` : '';
  return apiFetch(`/api/queues/${encodeURIComponent(queue)}/jobs${query}`, z.array(JobSchema));
}

export function retryJob(id: string): Promise<void> {
  return apiFetch(`/api/jobs/${id}/retry`, z.void(), { method: 'POST' });
}

export function purgeJob(id: string): Promise<void> {
  return apiFetch(`/api/jobs/${id}`, z.void(), { method: 'DELETE' });
}
