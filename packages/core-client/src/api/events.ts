import { z } from 'zod';
import { apiFetch } from './client.js';
import {
  EventSchema,
  EventSummarySchema,
  type Event,
  type EventSummary,
  type CreateEventPayload,
  type UpdateEventPayload,
} from '../schemas/event.js';

export function listEvents(): Promise<EventSummary[]> {
  return apiFetch('/api/events', z.array(EventSummarySchema));
}

export function getEvent(id: string): Promise<Event> {
  return apiFetch(`/api/events/${id}`, EventSchema);
}

export function createEvent(payload: CreateEventPayload): Promise<Event> {
  return apiFetch('/api/events', EventSchema, { method: 'POST', body: payload });
}

export function updateEvent(id: string, payload: UpdateEventPayload): Promise<Event> {
  return apiFetch(`/api/events/${id}`, EventSchema, { method: 'PUT', body: payload });
}

export function deleteEvent(id: string): Promise<void> {
  return apiFetch(`/api/events/${id}`, z.void(), { method: 'DELETE' });
}

/** Writes one .pptx per Bible reference into the configured Bible slide folder. */
export function createEventSlides(id: string): Promise<{ files: string[] }> {
  return apiFetch(`/api/events/${id}/slides`, z.object({ files: z.array(z.string()) }), {
    method: 'POST',
  });
}
