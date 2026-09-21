import { SvelteSet } from 'svelte/reactivity';
import type { ToastTone } from '@metocast/design-system';

// Tier drives both the toast accent and the bell's worst-active mark. `live` is the
// calmest positive signal, `error` the loudest; the order here is the severity order.
export type Tier = ToastTone; // 'live' | 'ok' | 'warn' | 'error'
const RANK: Record<Tier, number> = { live: 0, ok: 1, warn: 2, error: 3 };

export interface NotifAction {
  label: string;
  primary?: boolean;
  /** Return the promise to have the toast's button animate until the work settles. */
  run: () => void | Promise<unknown>;
}

export interface Notification {
  id: number;
  tier: Tier;
  kind: string;
  source: string;
  title: string;
  body?: string;
  /** Technical line, set in the mono face. */
  detail?: string;
  /** Renders `body` in the mono face — for a raw path, code, or identifier a serif
   * numeral could be misread in (a "0" as "o"). */
  mono?: boolean;
  /** Numbered remediation steps, disclosed on demand. */
  remediation?: string[];
  actions?: NotifAction[];
  /** A settled/in-progress chip beside the source — "reconnecting", "offline". */
  state?: string;
  /** Each affected source, when several are folded into one notification. */
  group?: { source: string; label: string }[];
  /** Persistent items never self-dismiss from the transient rail. */
  persistent: boolean;
  /** An unresolved machine condition, distinct from an unread history item. */
  issue?: boolean;
  /** Dedupe/resolve handle — a connector id, so recovery clears its own failure. */
  key?: string;
  /** simple-icons path for the source's brand logo, masked into the Glyph tile. */
  brand?: string;
  /** Only for a mark that is not drawn in `simple-icons`' 24x24 box. */
  brandBox?: string;
  /** Set once the source recovers; a resolved failure no longer blocks a fresh one. */
  resolved?: boolean;
  createdAt: number;
  railDismissed: boolean;
  read: boolean;
}

let items = $state<Notification[]>([]);
/** Keys deleted from the centre, for notifications that track an ongoing machine
 *  condition (`state` is set — a connector that is still down). Recovery lifts the
 *  mute, so the next genuine failure is news again. Toasts answering a click never
 *  mute: pressing Copy twice has to report twice. */
const muted = new SvelteSet<string>();
let seq = 0;
const RAIL_CAP = 3;

function worst(list: Notification[]): Tier {
  return list.reduce<Tier>((t, n) => (RANK[n.tier] > RANK[t] ? n.tier : t), 'live');
}

function isActiveIssue(n: Notification): boolean {
  return n.issue === true && !n.resolved;
}

/** The transient rail: newest first, capped, still-showing items only. */
export function railItems(): Notification[] {
  return items
    .filter((n) => !n.railDismissed)
    .slice(-RAIL_CAP)
    .reverse();
}

/** The centre's full history, newest first. */
export function allNotifications(): Notification[] {
  return items.slice().reverse();
}

/** Unresolved conditions, separate from ordinary notification history. */
export function activeIssues(): Notification[] {
  return items.filter(isActiveIssue).reverse();
}

/** Everything the user may clear without hiding an unresolved condition. */
export function historyNotifications(): Notification[] {
  return items.filter((n) => !isActiveIssue(n)).reverse();
}

export function activeIssueCount(): number {
  return items.filter(isActiveIssue).length;
}

export function unreadCount(): number {
  return items.filter((n) => !n.read).length;
}

/** Unread history only, so the bell label does not count active issues twice. */
export function unreadHistoryCount(): number {
  return items.filter((n) => !isActiveIssue(n) && !n.read).length;
}

/** Active health wins over read state; otherwise show the worst unread tier. */
export function topTier(): Tier | 'off' {
  if (items.some(isActiveIssue)) return 'error';
  const unread = items.filter((n) => !n.read);
  return unread.length ? worst(unread) : 'off';
}

export function notify(
  n: Omit<Notification, 'id' | 'createdAt' | 'railDismissed' | 'read' | 'persistent'> & {
    persistent?: boolean;
  },
): number {
  if (n.key) {
    // A connector that is still down re-broadcasts the same failure every few
    // seconds. Without the mute the card the operator just deleted returns before
    // the sheet has finished closing, so it cannot be cleared at all.
    if (muted.has(n.key)) return 0;
    const active = items.find((i) => i.key === n.key && !i.resolved);
    if (active) {
      // A repeated status packet refreshes the diagnosis without undoing the user's
      // decision to minimize/read this still-unresolved condition.
      const railDismissed = active.railDismissed;
      const read = active.read;
      Object.assign(active, n, { railDismissed, read });
      return active.id;
    }
  }
  const id = ++seq;
  const persistent = n.persistent ?? n.tier === 'error';
  // One connector issue may interrupt at a time. Further issues in the same burst
  // start in the compact bell/centre state and never form a delayed popup queue.
  const railDismissed =
    n.issue === true && items.some((item) => isActiveIssue(item) && !item.railDismissed);
  items.push({ ...n, persistent, id, createdAt: Date.now(), railDismissed, read: false });
  if (!persistent) setTimeout(() => dismissRail(id), 6000);
  return id;
}

/** Clears a keyed failure when its source recovers, and announces the recovery. */
export function resolveByKey(
  key: string,
  recovery?: Pick<Notification, 'kind' | 'source' | 'title' | 'body'>,
): void {
  muted.delete(key);
  // Do not leave a red failure card behind in history after the condition recovers.
  items = items.filter((n) => n.key !== key);
  if (recovery) notify({ ...recovery, tier: 'ok', persistent: false });
}

/** Removes a notification from the toast rail; it stays in the centre. */
export function dismissRail(id: number): void {
  const n = items.find((i) => i.id === id);
  if (n) n.railDismissed = true;
}

/** Deletes a notification outright — the centre's dismiss. */
export function dismiss(id: number): void {
  const n = items.find((i) => i.id === id);
  if (n && isActiveIssue(n)) {
    n.railDismissed = true;
    return;
  }
  if (n?.key && n.state) muted.add(n.key);
  items = items.filter((i) => i.id !== id);
}

export function markAllRead(): void {
  for (const n of items) n.read = true;
}

export function clearAll(): void {
  items = items.filter(isActiveIssue);
}

// ── Notification centre open state ───────────────────────────────────────────
let centreOpen = $state(false);
export function isCentreOpen(): boolean {
  return centreOpen;
}
export function openCentre(): void {
  centreOpen = true;
  markAllRead();
}
export function closeCentre(): void {
  centreOpen = false;
}

// ── Back-compat shims for the connectors page, which speaks the old toast API ──
export interface ToastItem {
  kind: string;
  source: string;
  title: string;
  body?: string;
  tone: ToastTone;
}
export function toasts(): Notification[] {
  return railItems();
}
export function pushToast(t: ToastItem): void {
  notify({ ...t, tier: t.tone, key: `${t.source}:${t.title}` });
}
export function dismissToast(id: number): void {
  dismissRail(id);
}
