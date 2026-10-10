import { z } from 'zod';
import { apiFetch } from './client.js';
import { sendWsCommand } from '../ws/client.js';
import {
  AutomationDraftSchema,
  AutomationListSchema,
  AutomationRunSchema,
  AutomationSchema,
} from '../schemas/automations.js';
import type {
  Automation,
  AutomationDraft,
  AutomationList,
  AutomationRun,
} from '../schemas/automations.js';

/** The rules and the master switch. Changes arrive to every client as `automations.changed`. */
export function listAutomations(): Promise<AutomationList> {
  return apiFetch('/api/automations', AutomationListSchema);
}

/** Rejects with an `ApiError` (400, `{ error }`) for a rule that breaks a limit. */
export function createAutomation(draft: AutomationDraft): Promise<Automation> {
  return apiFetch('/api/automations', AutomationSchema, {
    method: 'POST',
    body: AutomationDraftSchema.parse(draft),
  });
}

/** Replaces the whole rule. */
export function updateAutomation(id: string, draft: AutomationDraft): Promise<Automation> {
  return apiFetch(`/api/automations/${id}`, AutomationSchema, {
    method: 'PUT',
    body: AutomationDraftSchema.parse(draft),
  });
}

export function deleteAutomation(id: string): Promise<void> {
  return apiFetch(`/api/automations/${id}`, z.void(), { method: 'DELETE' });
}

/**
 * Runs the rule's actions now, ignoring its cooldown, its event check and the master switch. It
 * really records and really moves cameras. Resolves with the finished run.
 */
export function runAutomation(id: string): Promise<AutomationRun> {
  return apiFetch(`/api/automations/${id}/run`, AutomationRunSchema, { method: 'POST' });
}

/** The last 50 runs, newest first. */
export function listAutomationRuns(): Promise<AutomationRun[]> {
  return apiFetch('/api/automations/runs', z.array(AutomationRunSchema));
}

/** The master switch. While it is off nothing fires; "Run now" still works. */
export async function setAutomationsArmed(armed: boolean): Promise<void> {
  await apiFetch('/api/automations/armed', z.object({ armed: z.boolean() }), {
    method: 'PUT',
    body: { armed },
  });
}

/** Asks the core for the rules, answered as `automations.changed`. */
export function requestAutomations(): boolean {
  return sendWsCommand('automations.list');
}

/** Runs a rule now without waiting for it; the run arrives as `automation.run`. */
export function sendRunAutomation(id: string): boolean {
  return sendWsCommand('automations.run', { id });
}

export function sendAutomationsArmed(armed: boolean): boolean {
  return sendWsCommand('automations.set_armed', { armed });
}
