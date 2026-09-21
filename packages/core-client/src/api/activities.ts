import { z } from 'zod';
import { apiFetch } from './client.js';
import {
  EventActivitySchema,
  type EventActivity,
  type CreateEventActivityPayload,
} from '../schemas/event.js';

export function listActivities(eventId: string): Promise<EventActivity[]> {
  return apiFetch(`/api/events/${eventId}/activities`, z.array(EventActivitySchema));
}

export function createActivity(
  eventId: string,
  payload: CreateEventActivityPayload,
): Promise<EventActivity> {
  return apiFetch(`/api/events/${eventId}/activities`, EventActivitySchema, {
    method: 'POST',
    body: payload,
  });
}

export function deleteActivity(eventId: string, activityId: string): Promise<void> {
  return apiFetch(`/api/events/${eventId}/activities/${activityId}`, z.undefined(), {
    method: 'DELETE',
  });
}
