import { writable } from 'svelte/store';

/** The two announcement channels, rendered once by `LiveRegion` in the app shell. */
export const politeMessage = writable('');
export const assertiveMessage = writable('');

/**
 * Announce to assistive technology without moving focus. `assertive` interrupts and is for
 * things the user must hear now — a failure, a lost connection. Everything else is polite.
 */
export function announce(message: string, priority: 'polite' | 'assertive' = 'polite'): void {
  const channel = priority === 'assertive' ? assertiveMessage : politeMessage;
  // Re-announce an identical message by clearing first; a live region ignores an unchanged value.
  channel.set('');
  queueMicrotask(() => channel.set(message));
}
