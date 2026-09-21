#!/usr/bin/env node
// The check that the previous attempt did not have: a built component's values, diffed against
// the measurements they were taken from. A value that drifted from 14px to 12px stops being
// something a reviewer might notice and becomes a failure.
//
// Three rules, all mechanical:
//   1. Every component token maps to a measurement, and resolves to that measurement's value.
//      A difference is a failure unless fidelity.json records it as a deviation with a reason.
//   2. No literal value appears in a component's styles — every styled property resolves
//      through a token.
//   3. Every media query width is a declared breakpoint token value. CSS cannot read a custom
//      property inside a media query, so this is how a breakpoint token stays authoritative.
import { readFileSync, readdirSync, statSync } from 'node:fs';
import { join, relative } from 'node:path';
import postcss from 'postcss';
import { measurements, packageDir, primitives, root, tokensDir } from './tokens-lib.mjs';

const fidelity = JSON.parse(readFileSync(join(tokensDir, 'fidelity.json'), 'utf8'));
const failures = [];
const notes = [];

// ── Token resolution ────────────────────────────────────────────────────────────────────────
const flat = new Map();
(function collect(node, path = []) {
  for (const [key, value] of Object.entries(node)) {
    if (key.startsWith('$')) continue;
    if (value && typeof value === 'object' && '$value' in value) {
      flat.set([...path, key].join('-'), String(value.$value));
    } else if (value && typeof value === 'object') {
      collect(value, [...path, key]);
    }
  }
})(primitives);

/** `{space.14}` → `14px`, following aliases to a literal. */
function resolve(name, seen = new Set()) {
  if (seen.has(name)) return null;
  seen.add(name);
  const raw = flat.get(name);
  if (raw === undefined) return null;
  const alias = /^\{([^}]+)\}$/.exec(raw.trim());
  return alias ? resolve(alias[1].replace(/\./g, '-'), seen) : raw;
}

// ── Rule 1: component tokens match their measurements ───────────────────────────────────────
const LENGTH = /(-?\d*\.?\d+)(px|rem|em|ms|s|%|vh|vw|dvh|svh|lvh|vmin|vmax|ch|ex|fr|deg)?/gi;
const strip = (value) =>
  value.replace(
    /\b(?:color-mix|rgba?|hsla?|oklch|oklab|var|cubic-bezier|steps|linear)\([^()]*(?:\([^()]*\)[^()]*)*\)/g,
    ' ',
  );

const byKey = new Map();
for (const row of measurements.rows) {
  byKey.set(`${row.source}|${row.selector}|${row.property}|${row.breakpoint}`, row);
}

/** The nth measurement inside a declaration, so a shorthand's parts can each carry a token. */
function measuredValue(row, index = 0) {
  const parts = [...strip(row.value).matchAll(LENGTH)].map(([, num, unit]) =>
    unit ? `${parseFloat(num)}${unit}` : String(parseFloat(num)),
  );
  return parts[index] ?? null;
}

let compared = 0;
for (const [component, entry] of Object.entries(fidelity.components)) {
  for (const [token, spec] of Object.entries(entry.tokens ?? {})) {
    const deviation = entry.deviations?.[token];
    const implemented = resolve(token);
    if (implemented === null) {
      failures.push(
        `${component}: --${token} is mapped to a measurement but is not a declared token`,
      );
      continue;
    }
    const key = `${entry.source}|${spec.selector}|${spec.property}|${spec.breakpoint ?? 'base'}`;
    const row = byKey.get(key);
    if (!row) {
      failures.push(
        `${component}: --${token} cites ${entry.source} › \`${spec.selector}\` › ${spec.property}, which the reference does not declare`,
      );
      continue;
    }
    const measured = spec.literal ?? measuredValue(row, spec.index ?? 0);
    compared += 1;
    if (measured === implemented) {
      if (deviation) {
        failures.push(
          `${component}: --${token} records a deviation but matches its measurement — remove the deviation`,
        );
      }
      continue;
    }
    if (deviation)
      notes.push(`${component}: --${token} is ${implemented}, measured ${measured} — ${deviation}`);
    else {
      failures.push(
        `${component}: --${token} drifted — implemented ${implemented}, measured ${measured} at ${entry.source} › \`${spec.selector}\` › ${spec.property}`,
      );
    }
  }
}

// ── Rules 2 and 3: component styles ─────────────────────────────────────────────────────────
/** Properties whose values must resolve through a token rather than being written literally. */
const TOKENISED = new Set([
  'color',
  'background',
  'background-color',
  'border',
  'border-top',
  'border-right',
  'border-bottom',
  'border-left',
  'border-color',
  'border-width',
  'border-radius',
  'border-top-left-radius',
  'border-top-right-radius',
  'border-bottom-left-radius',
  'border-bottom-right-radius',
  'outline',
  'outline-color',
  'outline-width',
  'outline-offset',
  'padding',
  'padding-top',
  'padding-right',
  'padding-bottom',
  'padding-left',
  'margin',
  'margin-top',
  'margin-right',
  'margin-bottom',
  'margin-left',
  'gap',
  'row-gap',
  'column-gap',
  'width',
  'height',
  'min-width',
  'min-height',
  'max-width',
  'max-height',
  'font-family',
  'font-size',
  'font-weight',
  'line-height',
  'letter-spacing',
  'box-shadow',
  'z-index',
  'transition',
  'transition-duration',
  'animation',
  'animation-duration',
  'top',
  'right',
  'bottom',
  'left',
  'inset',
  'fill',
  'stroke',
]);

/**
 * What counts as a literal: a length, duration or colour written out rather than referenced.
 * Keywords, `0`, and the arithmetic inside a `calc()` are not measurements and need no token.
 */
const LITERAL = /^(#[0-9a-f]{3,8}|-?\d*\.?\d+(px|rem|em|%|ms|s|vh|vw|dvh|svh|lvh|ch|ex))$/i;
const COLOR_FUNCTION = /\b(rgba?|hsla?|oklch|oklab)\(/i;
/**
 * `100%` means "all of the parent" and `50%` on an offset means "centred" — both are layout
 * instructions rather than measurements. `50%` on a radius is still checked, because there it is
 * a shape decision the scale has a token for.
 */
const LAYOUT_PERCENT = new Set(['100%', '50%']);
const LAYOUT_PROPERTY = /^(width|height|min-|max-|top|right|bottom|left|inset|flex-basis)/;
/**
 * The WCAG visually-hidden recipe is a fixed set of values with one correct answer; treating its
 * 1px clip box as a design decision would put a meaningless token in the scale.
 */
const HIDDEN_IDIOM = /\.(visually-hidden|sr-only|table-head)\b/;

function walk(dir, out = []) {
  for (const entry of readdirSync(dir)) {
    const full = join(dir, entry);
    if (statSync(full).isDirectory()) walk(full, out);
    else if (entry.endsWith('.svelte')) out.push(full);
  }
  return out;
}

const breakpointWidths = new Set(
  Object.values(primitives.bp ?? {})
    .filter((token) => token && typeof token === 'object' && '$value' in token)
    .map((token) => String(token.$value)),
);

const srcDir = join(packageDir, 'src');
for (const file of walk(srcDir)) {
  const source = readFileSync(file, 'utf8');
  const open = /<style(\s[^>]*)?>/.exec(source);
  if (!open) continue;
  const start = open.index + open[0].length;
  const css = source.slice(start, source.indexOf('</style>', start));
  const where = relative(root, file);

  const ast = postcss.parse(css, { from: file });
  ast.walkDecls((decl) => {
    if (!TOKENISED.has(decl.prop)) return;
    if (decl.parent.type === 'rule' && HIDDEN_IDIOM.test(decl.parent.selector)) return;
    // Whatever a var() resolves to has already been checked as a token.
    const remainder = decl.value.replace(/var\([^()]*(?:\([^()]*\)[^()]*)*\)/g, ' ').trim();
    const leftovers = remainder
      .split(/[\s,/()*+]+/)
      .filter(Boolean)
      .filter(
        (part) =>
          LITERAL.test(part) && !(LAYOUT_PERCENT.has(part) && LAYOUT_PROPERTY.test(decl.prop)),
      );
    if (COLOR_FUNCTION.test(remainder)) leftovers.push(remainder.match(COLOR_FUNCTION)[0]);
    if (leftovers.length > 0) {
      failures.push(
        `${where}:${decl.source?.start?.line}: \`${decl.prop}: ${decl.value}\` uses the literal ${leftovers.join(' ')} — every value must resolve through a token`,
      );
    }
  });
  ast.walkAtRules('media', (at) => {
    for (const [, width] of at.params.matchAll(/(?:min|max)-width:\s*(\d+px)/g)) {
      if (!breakpointWidths.has(width)) {
        failures.push(
          `${where}: media query at ${width} is not a declared breakpoint token (${[...breakpointWidths].join(', ')})`,
        );
      }
    }
  });
}

// ── Report ──────────────────────────────────────────────────────────────────────────────────
for (const note of notes) console.log(`  deviation  ${note}`);
if (failures.length > 0) {
  console.error(`\nFidelity check failed (${failures.length}):`);
  for (const failure of failures) console.error(`  ${failure}`);
  process.exit(1);
}
console.log(
  `\nFidelity passed: ${compared} component tokens diffed against their measurements, ${notes.length} recorded deviations.`,
);
