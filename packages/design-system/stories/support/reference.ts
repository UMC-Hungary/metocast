import fidelity from '../../tokens/fidelity.json';
import measurements from '../../tokens/measurements.json';
import tokens from '../../tokens/tokens.json';

/** The shape `tokens/fidelity.json` holds for one component. */
interface FidelityEntry {
  source: string;
  tokens?: Record<
    string,
    { selector: string; property: string; index?: number; breakpoint?: string }
  >;
  deviations?: Record<string, string>;
}

interface Row {
  token: string;
  property: string;
  selector: string;
  measured: string;
  implemented: string;
  deviation?: string;
}

const flat = new Map<string, string>();
(function collect(node: Record<string, unknown>, path: string[] = []) {
  for (const [key, value] of Object.entries(node)) {
    if (key.startsWith('$')) continue;
    if (value && typeof value === 'object' && '$value' in (value as object)) {
      flat.set([...path, key].join('-'), String((value as { $value: unknown }).$value));
    } else if (value && typeof value === 'object') {
      collect(value as Record<string, unknown>, [...path, key]);
    }
  }
})(tokens as unknown as Record<string, unknown>);

function resolve(name: string, seen = new Set<string>()): string {
  if (seen.has(name)) return '↺';
  seen.add(name);
  const raw = flat.get(name);
  if (raw === undefined) return '—';
  const alias = /^\{([^}]+)\}$/.exec(raw.trim());
  return alias ? resolve(alias[1].replace(/\./g, '-'), seen) : raw;
}

const LENGTH = /(-?\d*\.?\d+)(px|rem|em|ms|s|%|vh|vw|dvh|svh|lvh|vmin|vmax|ch|ex|fr|deg)?/gi;
const strip = (value: string) =>
  value.replace(
    /\b(?:color-mix|rgba?|hsla?|oklch|oklab|var|cubic-bezier|steps|linear)\([^()]*(?:\([^()]*\)[^()]*)*\)/g,
    ' ',
  );

const byKey = new Map<string, { value: string }>();
for (const row of measurements.rows) {
  byKey.set(`${row.source}|${row.selector}|${row.property}|${row.breakpoint}`, row);
}

/**
 * The recorded reference measurements for a component, so the catalog shows each value beside the
 * declaration it came from. Same inputs as `scripts/check-fidelity.mjs`, so what a reviewer reads
 * here is what continuous integration enforces.
 */
export function referenceFor(component: string): { source: string; rows: Row[] } | null {
  const entry = (fidelity.components as Record<string, FidelityEntry | undefined>)[component];
  if (!entry) return null;
  const rows: Row[] = [];
  for (const [token, spec] of Object.entries(entry.tokens ?? {})) {
    const row = byKey.get(
      `${entry.source}|${spec.selector}|${spec.property}|${spec.breakpoint ?? 'base'}`,
    );
    const parts = row
      ? [...strip(row.value).matchAll(LENGTH)].map(([, num, unit]) =>
          unit ? `${parseFloat(num)}${unit}` : String(parseFloat(num)),
        )
      : [];
    rows.push({
      token,
      property: spec.property,
      selector: spec.selector,
      measured: parts[spec.index ?? 0] ?? '—',
      implemented: resolve(token),
      deviation: entry.deviations?.[token],
    });
  }
  return { source: entry.source, rows };
}
