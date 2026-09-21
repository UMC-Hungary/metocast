// Date/time formatting for the screens, localised through Intl. The core hands us
// ISO strings; every screen renders them the same way from here.

function fmt(iso: string, locale: string, opts: Intl.DateTimeFormatOptions): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return '';
  return new Intl.DateTimeFormat(locale, opts).format(d);
}

export function monthAbbr(iso: string, locale = 'en'): string {
  return fmt(iso, locale, { month: 'short' }).toUpperCase().replace('.', '');
}

export function dayNum(iso: string, locale = 'en'): string {
  return fmt(iso, locale, { day: 'numeric' });
}

export function timeShort(iso: string, locale = 'en'): string {
  return fmt(iso, locale, { hour: '2-digit', minute: '2-digit' });
}

export function dateLong(iso: string, locale = 'en'): string {
  return fmt(iso, locale, { weekday: 'short', day: 'numeric', month: 'long', year: 'numeric' });
}

export function dateTimeLabel(iso: string, locale = 'en'): string {
  return fmt(iso, locale, {
    weekday: 'long',
    day: 'numeric',
    month: 'long',
    hour: '2-digit',
    minute: '2-digit',
  });
}

/** Elapsed time as `HH:MM:SS`, from a count of seconds. */
export function clock(seconds: number): string {
  const s = Math.max(0, Math.round(seconds));
  const pad = (n: number) => String(n).padStart(2, '0');
  return `${pad(Math.floor(s / 3600))}:${pad(Math.floor((s % 3600) / 60))}:${pad(s % 60)}`;
}

/** "3 min ago" style relative age, from an ISO timestamp. */
export function relAge(iso: string, locale = 'en'): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return '';
  const secs = Math.round((d.getTime() - Date.now()) / 1000);
  const rtf = new Intl.RelativeTimeFormat(locale, { numeric: 'auto' });
  const abs = Math.abs(secs);
  if (abs < 60) return rtf.format(Math.round(secs), 'second');
  if (abs < 3600) return rtf.format(Math.round(secs / 60), 'minute');
  return rtf.format(Math.round(secs / 3600), 'hour');
}

/** For the `<input type="date">` / `time` value bindings, which want split fields. */
export function toDateInput(iso: string): string {
  const d = new Date(iso);
  return Number.isNaN(d.getTime()) ? '' : d.toISOString().slice(0, 10);
}

export function toTimeInput(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return '';
  return `${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`;
}

/** Combines the two native inputs back into an ISO string the core accepts. */
export function fromDateTimeInput(date: string, time: string): string {
  if (!date) return '';
  return new Date(`${date}T${time || '00:00'}`).toISOString();
}

/** What an event is called on screen: the composed title when it exists, else the raw one. */
export function eventTitle(e: { title: string; computedTitle: string }): string {
  return e.computedTitle || e.title;
}
