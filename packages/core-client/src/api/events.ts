import { z } from 'zod';
import { apiFetch } from './client.js';
import { sendWsCommand } from '../ws/client.js';
import {
  EventOrderSchema,
  type OrderItem,
  EventSchema,
  EventSummarySchema,
  type Event,
  type EventSummary,
  type CreateEventPayload,
  type UpdateEventPayload,
  type BroadcastPlatform,
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

/**
 * Takes the event's broadcast on `platform` live (`start`) or ends it (`stop`). Destructive.
 * A refusal is an `ApiError` whose `code` says why: `encoder_not_streaming`, `not_live`, …
 * Every client then gets `event.changed`.
 */
export async function controlEventStream(
  id: string,
  platform: BroadcastPlatform,
  action: 'start' | 'stop',
): Promise<void> {
  await apiFetch(
    `/api/events/${id}/stream/${platform}/${action}`,
    z.object({ ok: z.literal(true) }),
    {
      method: 'POST',
    },
  );
}

// ── Order of Service ──────────────────────────────────────────────────────────

export function getEventOrder(id: string): Promise<OrderItem[]> {
  return apiFetch(`/api/events/${id}/order`, EventOrderSchema).then((order) => order.items);
}

/** Replaces the whole list. Keep each step's `id` so the step on screen stays current. */
export function setEventOrder(id: string, items: OrderItem[]): Promise<OrderItem[]> {
  return apiFetch(`/api/events/${id}/order`, EventOrderSchema, {
    method: 'PUT',
    body: { items },
  }).then((order) => order.items);
}

/** Shows a step on the output screen; the result arrives as `order.state`. */
export function orderGo(eventId: string, itemId: string): boolean {
  return sendWsCommand('order.go', { event_id: eventId, item_id: itemId });
}

/** Moves from the step on screen. With none, `eventId` names the event and `next` starts it. */
export function orderStep(direction: 'next' | 'previous', eventId: string | null = null): boolean {
  return sendWsCommand(`order.${direction}`, { event_id: eventId });
}
