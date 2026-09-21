/**
 * Renders the published event title from a user-editable template.
 *
 * Placeholders are `{name}`, with an optional pipe that only `date` reads:
 * `{date|YYYY.MM.DD.}`. Text wrapped in `[ ... ]` is dropped whole when any
 * placeholder inside it is empty, so a missing leckió takes its label with it.
 */

/** Kept in step with `DEFAULT_TITLE_TEMPLATE` in src-tauri/src/models/event.rs. */
export const DEFAULT_TITLE_TEMPLATE =
  '{date|YYYY.MM.DD.} {title}[ | Textus: {textus}][ Lekció: {leckio}][ | {speaker}]';

export const TITLE_VARIABLES = ['date', 'title', 'textus', 'leckio', 'speaker'] as const;

export type TitleVariable = (typeof TITLE_VARIABLES)[number];

export interface TitleValues {
  date: Date | null;
  title: string;
  textus: string;
  leckio: string;
  speaker: string;
}

const DEFAULT_DATE_PATTERN = 'YYYY.MM.DD.';

// Longest first: the alternation is leftmost-wins, so YYYY must beat YY.
const DATE_TOKENS = /YYYY|MMMM|dddd|MM|DD|HH|mm|YY|M|D/g;

const PLACEHOLDER = /\{(\w+)(?:\|([^}]*))?\}/g;

const OPTIONAL_GROUP = /\[([^\]]*)\]/g;

function formatDate(date: Date, pattern: string, locale: string): string {
  const pad = (n: number) => String(n).padStart(2, '0');
  const named = (option: Intl.DateTimeFormatOptions) =>
    new Intl.DateTimeFormat(locale, option).format(date);
  const parts: Record<string, string> = {
    YYYY: String(date.getFullYear()),
    YY: String(date.getFullYear()).slice(-2),
    MMMM: named({ month: 'long' }),
    dddd: named({ weekday: 'long' }),
    MM: pad(date.getMonth() + 1),
    DD: pad(date.getDate()),
    M: String(date.getMonth() + 1),
    D: String(date.getDate()),
    HH: pad(date.getHours()),
    mm: pad(date.getMinutes()),
  };
  return pattern.replace(DATE_TOKENS, (token) => parts[token] ?? token);
}

export function renderTitle(template: string, values: TitleValues, locale = 'en'): string {
  const read = (name: string, pattern?: string): string => {
    if (name === 'date') {
      return values.date ? formatDate(values.date, pattern || DEFAULT_DATE_PATTERN, locale) : '';
    }
    const value = values[name as TitleVariable];
    return typeof value === 'string' ? value.trim() : '';
  };

  const fill = (text: string) =>
    text.replace(PLACEHOLDER, (_, name: string, pattern?: string) => read(name, pattern));

  const kept = template.replace(OPTIONAL_GROUP, (_, inner: string) => {
    const names = [...inner.matchAll(PLACEHOLDER)].flatMap((m) => (m[1] ? [m[1]] : []));
    return names.every((name) => read(name)) ? inner : '';
  });

  return fill(kept).replace(/\s+/g, ' ').trim();
}
