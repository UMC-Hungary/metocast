#!/usr/bin/env node
// Extracts every CSS declaration from the design reference so the token scale can be derived
// from what the reference actually uses rather than from a grid.
//
// Emits:
//   packages/design-system/tokens/measurements.json  machine-readable, consumed by the scale
//                                                    derivation and the fidelity diff
//   packages/design-system/tokens/measurements.md    committed, human-readable, grouped by source
//
// `--check` re-runs against the committed output and fails on any difference, which is what
// makes the record trustworthy as a source rather than a snapshot someone edited by hand.
import { readFileSync, readdirSync, statSync, writeFileSync } from 'node:fs';
import { dirname, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import postcss from 'postcss';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const outDir = join(root, 'packages/design-system/tokens');
const jsonOut = join(outDir, 'measurements.json');
const mdOut = join(outDir, 'measurements.md');

const referenceRoot = resolve(
  process.env.SANCTUM_REFERENCE ??
    join(process.env.HOME ?? '', 'workspace/ui/sermon-helper-svelte'),
);

/**
 * The reference's own reflow thresholds, named after what it actually declares. The change's
 * design named two (760 and 1360); the extraction found four, and the extraction is the
 * authority. Any query that is not a bare single min/max-width is recorded verbatim so it
 * cannot be mistaken for one of these.
 */
const BREAKPOINTS = {
  'max-width:420': 'narrow',
  'min-width:760': 'md',
  'min-width:980': 'lg',
  'min-width:1360': 'xl',
};

function walk(dir, out = []) {
  for (const entry of readdirSync(dir)) {
    if (entry === 'node_modules' || entry === 'dist' || entry.startsWith('.')) continue;
    const full = join(dir, entry);
    if (statSync(full).isDirectory()) walk(full, out);
    else if (entry.endsWith('.svelte') || entry.endsWith('.css')) out.push(full);
  }
  return out;
}

/** The `<style>` body of a Svelte SFC, with its offset so line numbers stay true to the file. */
function styleBlock(source) {
  const open = /<style(\s[^>]*)?>/.exec(source);
  if (!open) return null;
  const start = open.index + open[0].length;
  const end = source.indexOf('</style>', start);
  if (end === -1) return null;
  return { css: source.slice(start, end), line: source.slice(0, start).split('\n').length - 1 };
}

/**
 * Names the breakpoint a rule sits under. `base` means it holds at every width; `md`/`lg` are
 * the reference's own thresholds; anything else is kept verbatim so it cannot be mistaken for one.
 */
function breakpointOf(rule) {
  const queries = [];
  for (let node = rule.parent; node; node = node.parent) {
    if (node.type === 'atrule' && node.name === 'media') queries.unshift(node.params.trim());
  }
  if (queries.length === 0) return { breakpoint: 'base', media: null };
  const media = queries.join(' and ');
  const single = /^\((min|max)-width:\s*(\d+)px\)$/.exec(media);
  const named = single && BREAKPOINTS[`${single[1]}-width:${single[2]}`];
  return { breakpoint: named ?? media, media };
}

function collect(file) {
  const source = readFileSync(file, 'utf8');
  const isSvelte = file.endsWith('.svelte');
  const block = isSvelte ? styleBlock(source) : { css: source, line: 0 };
  if (!block) return [];

  const rows = [];
  const rootRelative = relative(referenceRoot, file);
  let ast;
  try {
    ast = postcss.parse(block.css, { from: file });
  } catch (error) {
    throw new Error(`Failed to parse styles in ${rootRelative}: ${error.message}`);
  }

  ast.walkDecls((decl) => {
    const rule = decl.parent;
    if (rule.type !== 'rule') return;
    const { breakpoint, media } = breakpointOf(rule);
    rows.push({
      source: rootRelative,
      selector: rule.selector.replace(/\s+/g, ' ').trim(),
      property: decl.prop,
      value: decl.value.replace(/\s+/g, ' ').trim(),
      breakpoint,
      media,
      line: block.line + (decl.source?.start?.line ?? 0),
    });
  });

  // Keyframes carry the motion vocabulary; without them the duration set reads as arbitrary.
  ast.walkAtRules('keyframes', (at) => {
    at.walkDecls((decl) => {
      rows.push({
        source: rootRelative,
        selector: `@keyframes ${at.params} { ${decl.parent.selector} }`,
        property: decl.prop,
        value: decl.value.replace(/\s+/g, ' ').trim(),
        breakpoint: 'base',
        media: null,
        line: block.line + (decl.source?.start?.line ?? 0),
      });
    });
  });

  return rows;
}

/** Custom-property declarations under a given selector — the reference's palette lives here. */
function paletteFor(rows, selector) {
  const out = {};
  for (const row of rows) {
    if (row.selector === selector && row.property.startsWith('--')) out[row.property] = row.value;
  }
  return out;
}

// Units are captured even when they are not wanted, so an unwanted one is filtered rather than
// leaving its number looking unitless — `100dvh` must not read as the bare number 100.
const LENGTH = /(-?\d*\.?\d+)(px|rem|em|ms|s|%|vh|vw|dvh|svh|lvh|vmin|vmax|ch|ex|fr|deg)?/gi;

/**
 * Numbers inside a colour function are ratios and numbers inside an easing curve are control
 * points — neither is a measurement. Strip those, and `var()` references, before reading values
 * off a declaration. `calc()` is left alone because its operands often are measurements.
 */
function measurable(value) {
  return value.replace(
    /\b(?:color-mix|rgba?|hsla?|oklch|oklab|var|cubic-bezier|steps|linear)\([^()]*(?:\([^()]*\)[^()]*)*\)/g,
    ' ',
  );
}

/**
 * The distinct value set per category. This set — not an arithmetic progression — is the scale.
 * Only properties whose values are scale-bearing are counted; `flex`, `z-index` and the like
 * carry numbers that are not measurements.
 */
const CATEGORIES = {
  spacing: [
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
    'inset',
    'top',
    'right',
    'bottom',
    'left',
  ],
  sizing: ['width', 'height', 'min-width', 'min-height', 'max-width', 'max-height', 'flex-basis'],
  type: ['font-size'],
  leading: ['line-height'],
  tracking: ['letter-spacing'],
  weight: ['font-weight'],
  radius: [
    'border-radius',
    'border-top-left-radius',
    'border-top-right-radius',
    'border-bottom-left-radius',
    'border-bottom-right-radius',
  ],
  border: [
    'border',
    'border-top',
    'border-right',
    'border-bottom',
    'border-left',
    'border-width',
    'outline',
    'outline-width',
  ],
  duration: ['transition', 'transition-duration', 'animation', 'animation-duration'],
  layer: ['z-index'],
};

function valueSets(rows) {
  const sets = {};
  for (const [category, properties] of Object.entries(CATEGORIES)) {
    const seen = new Map();
    for (const row of rows) {
      if (!properties.includes(row.property)) continue;
      // Unitless is the only legal form for weight, line-height and z-index; for everything else
      // the sole unitless measurement is `0`, which is a value like any other.
      const unitless = ['weight', 'leading', 'layer'].includes(category);
      const units = category === 'duration' ? ['ms', 's'] : ['px', 'rem', 'em', '%'];
      for (const [, num, unit] of measurable(row.value).matchAll(LENGTH)) {
        if (unit ? unitless || !units.includes(unit) : !unitless && parseFloat(num) !== 0) continue;
        // `.8px` and `0.8px` are one value, not two.
        const token = `${parseFloat(num)}${unit ?? ''}`;
        if (!seen.has(token)) seen.set(token, new Set());
        seen.get(token).add(`${row.source}:${row.selector}`);
      }
    }
    sets[category] = [...seen.entries()]
      .sort((a, b) => (parseFloat(a[0]) || 0) - (parseFloat(b[0]) || 0))
      .map(([value, uses]) => ({ value, uses: uses.size, sources: [...uses].sort() }));
  }
  return sets;
}

/** Properties a component overrides inside a media query — the only responsive tokens allowed. */
function responsiveOverrides(rows) {
  const base = new Set();
  for (const row of rows) {
    if (row.breakpoint === 'base') base.add(`${row.source}|${row.selector}|${row.property}`);
  }
  return rows
    .filter((row) => row.breakpoint !== 'base')
    .map((row) => ({
      source: row.source,
      selector: row.selector,
      property: row.property,
      breakpoint: row.breakpoint,
      value: row.value,
      overridesBase: base.has(`${row.source}|${row.selector}|${row.property}`),
    }));
}

function markdown(record) {
  const lines = [
    '<!-- GENERATED by scripts/extract-reference.mjs — do not edit. Run `pnpm -F @metocast/design-system measure`. -->',
    '',
    '# Reference measurements',
    '',
    `Extracted from \`${record.reference}\` — ${record.files} stylesheets, ${record.rows.length} declarations.`,
    '',
    'Every value below is what the reference actually declares. The token scale is derived from',
    'this record and from nothing else: a value present here is present in the scale unrounded,',
    'and a value absent here does not enter the scale to complete a progression.',
    '',
    '## Palette',
    '',
    ...['light', 'dark'].flatMap((scheme) => [
      `### \`.theme-${scheme}\``,
      '',
      '| Custom property | Value |',
      '| --- | --- |',
      ...Object.entries(record.palette[scheme]).map(([k, v]) => `| \`${k}\` | \`${v}\` |`),
      '',
    ]),
    '## Distinct value sets',
    '',
    'These sets are the scale. Steps that look irregular are irregular in the reference.',
    '',
  ];

  for (const [category, values] of Object.entries(record.valueSets)) {
    lines.push(`### ${category}`, '', '| Value | Uses |', '| --- | --- |');
    for (const entry of values) lines.push(`| \`${entry.value}\` | ${entry.uses} |`);
    lines.push('');
  }

  lines.push(
    '## Responsive overrides',
    '',
    'The complete set of values the reference varies by viewport. A property absent from this',
    'table holds at every width, and a token for it must hold at every width too.',
    '',
    '| Source | Selector | Property | Breakpoint | Value | Overrides base |',
    '| --- | --- | --- | --- | --- | --- |',
    ...record.responsive.map(
      (r) =>
        `| \`${r.source}\` | \`${r.selector}\` | \`${r.property}\` | \`${r.breakpoint}\` | \`${r.value}\` | ${r.overridesBase ? 'yes' : 'no'} |`,
    ),
    '',
    '## Declarations by source',
    '',
  );

  const bySource = new Map();
  for (const row of record.rows) {
    if (!bySource.has(row.source)) bySource.set(row.source, []);
    bySource.get(row.source).push(row);
  }
  for (const [source, rows] of [...bySource].sort((a, b) => a[0].localeCompare(b[0]))) {
    lines.push(
      `### \`${source}\``,
      '',
      '| Selector | Property | Value | Breakpoint |',
      '| --- | --- | --- | --- |',
      ...rows.map(
        (r) =>
          `| \`${r.selector}\` | \`${r.property}\` | \`${r.value.replace(/\|/g, '\\|')}\` | \`${r.breakpoint}\` |`,
      ),
      '',
    );
  }

  return lines.join('\n');
}

/**
 * Read by hand off the three components the change names, so a silently dropped or misattributed
 * declaration fails loudly rather than becoming a token nobody questions. Each entry is
 * source → selector → property → value, taken from the file, not from this script's output.
 */
const HAND_CHECKED = {
  'components/primitives/Row.svelte': {
    'div, button': {
      display: 'flex',
      'align-items': 'flex-start',
      gap: '14px',
      padding: '14px 24px',
      'min-height': '56px',
      border: '0',
      'border-bottom': '1px solid var(--hairline)',
      transition: 'background 120ms',
      width: '100%',
      background: 'transparent',
      color: 'inherit',
      'text-align': 'left',
      'font-family': 'inherit',
    },
    '.last': { 'border-bottom': '0' },
    '.clickable': { cursor: 'pointer' },
    '.clickable:hover': { background: 'color-mix(in srgb, var(--ink) 3%, transparent)' },
    '.title': {
      'font-size': '15px',
      color: 'var(--ink)',
      'letter-spacing': '-0.1px',
      'font-weight': '400',
      'line-height': '1.24',
    },
    '.meta': { 'font-size': '12px', color: 'var(--ink-muted)', 'margin-top': '4px' },
    '.detail': { 'font-size': '14px', color: 'var(--ink-muted)', 'margin-top': '1px' },
  },
  'components/primitives/SectionLabel.svelte': {
    '.label': { padding: '28px 24px 10px' },
    'div, span': {
      'font-family': 'var(--font-mono)',
      'font-size': '10px',
      'letter-spacing': '2px',
      'text-transform': 'uppercase',
      color: 'var(--ink-muted)',
      'font-weight': '500',
    },
    span: { color: 'var(--ink-faint)', 'letter-spacing': '1px' },
  },
  'components/primitives/PageHeader.svelte': {
    header: { padding: '24px 24px 16px' },
    '.top': { 'min-height': '28px', 'margin-bottom': '14px' },
    '.actions': { gap: '8px' },
    '.eyebrow, .back': { 'font-size': '10px', 'letter-spacing': '2px' },
    h1: {
      'font-family': 'var(--font-serif)',
      'font-size': '44px',
      'line-height': '1.02',
      'font-weight': '500',
      'letter-spacing': '0',
    },
    '.title-row': { gap: '12px' },
  },
};

function verify(rows) {
  const index = new Map();
  for (const row of rows) {
    index.set(`${row.source}|${row.selector}|${row.property}|${row.breakpoint}`, row);
  }
  const problems = [];
  let checked = 0;
  for (const [source, selectors] of Object.entries(HAND_CHECKED)) {
    for (const [selector, declarations] of Object.entries(selectors)) {
      for (const [property, expected] of Object.entries(declarations)) {
        checked += 1;
        const found = index.get(`src/${source}|${selector}|${property}|base`);
        if (!found) problems.push(`missing ${source} › \`${selector}\` › ${property}`);
        else if (found.value !== expected) {
          problems.push(
            `wrong value ${source} › \`${selector}\` › ${property}: extracted "${found.value}", hand-checked "${expected}"`,
          );
        }
      }
    }
  }
  const duplicates = rows.length - index.size;
  return { problems, checked, duplicates };
}

const files = walk(referenceRoot === '' ? '.' : join(referenceRoot, 'src')).sort();
const rows = files.flatMap(collect);

const record = {
  reference: relative(root, referenceRoot),
  files: files.length,
  breakpoints: BREAKPOINTS,
  palette: {
    light: paletteFor(rows, '.theme-light'),
    dark: paletteFor(rows, '.theme-dark'),
    fonts: paletteFor(rows, ':root'),
  },
  valueSets: valueSets(rows),
  responsive: responsiveOverrides(rows),
  rows,
};

const sample = verify(rows);
if (sample.problems.length > 0) {
  console.error(`Extractor disagrees with the hand-checked sample (${sample.problems.length}):`);
  for (const problem of sample.problems) console.error(`  ${problem}`);
  process.exit(1);
}

const json = JSON.stringify(record, null, '\t') + '\n';
const md = markdown(record) + '\n';

if (process.argv.includes('--check')) {
  const stale = [
    [jsonOut, json],
    [mdOut, md],
  ].filter(([file, expected]) => readFileSync(file, 'utf8') !== expected);
  if (stale.length > 0) {
    console.error('Reference measurements are out of date:');
    for (const [file] of stale) console.error(`  ${relative(root, file)}`);
    console.error('Run `pnpm -F @metocast/design-system measure`.');
    process.exit(1);
  }
  console.log(
    `Measurements are current: ${rows.length} declarations across ${files.length} files.`,
  );
} else {
  writeFileSync(jsonOut, json);
  writeFileSync(mdOut, md);
  console.log(
    `Extracted ${rows.length} declarations from ${files.length} files in ${record.reference}.`,
  );
  console.log(
    `Hand-checked sample: ${sample.checked} declarations agree; ${sample.duplicates} duplicate keys.`,
  );
}
