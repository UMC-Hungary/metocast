import { writable, derived } from 'svelte/store';

export interface ConnectorError {
  id: string;
  connectorId: string;
  connectorName: string;
  message: string;
  infoMarkdown?: string;
  timestamp: Date;
}

export const connectorErrors = writable<ConnectorError[]>([]);

function randomId(): string {
  return Math.random().toString(36).slice(2) + Date.now().toString(36);
}

export function pushError(error: Omit<ConnectorError, 'id' | 'timestamp'>): void {
  connectorErrors.update((errs) => {
    // Replace existing error for the same connector (idempotent).
    const without = errs.filter((e) => e.connectorId !== error.connectorId);
    return [...without, { ...error, id: randomId(), timestamp: new Date() }];
  });
}

export function clearErrors(connectorId: string): void {
  connectorErrors.update((errs) => errs.filter((e) => e.connectorId !== connectorId));
}

export function clearError(id: string): void {
  connectorErrors.update((errs) => errs.filter((e) => e.id !== id));
}

/** Derived total count shown in the nav badge. */
export const errorCount = derived(connectorErrors, (errs) => errs.length);
