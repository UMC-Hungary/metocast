// Shared loading and colour maths for the token build and the token checks.
import { readFileSync, readdirSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

export const root = join(dirname(fileURLToPath(import.meta.url)), '..');
export const packageDir = join(root, 'packages/design-system');
export const tokensDir = join(packageDir, 'tokens');
export const outFile = join(tokensDir, 'generated/tokens.css');

const read = (p) => JSON.parse(readFileSync(p, 'utf8'));

export const primitives = read(join(tokensDir, 'tokens.json'));
export const pairings = read(join(tokensDir, 'pairings.json'));
export const measurements = read(join(tokensDir, 'measurements.json'));

/**
 * One design, so this is one file. It stays a directory read rather than a fixed filename so a
 * second theme pack would be caught by the check below instead of silently building.
 */
export function loadTheme() {
  const dir = join(tokensDir, 'themes');
  const files = readdirSync(dir).filter((f) => f.endsWith('.json'));
  if (files.length !== 1) {
    throw new Error(
      `Expected exactly one theme in ${dir} — the system ships one design with two schemes. Found: ${files.join(', ')}`,
    );
  }
  return read(join(dir, files[0]));
}

const isToken = (v) => v && typeof v === 'object' && '$value' in v;

/** DTCG alias `{a.b}` becomes `var(--a-b)`. */
export function resolveAliases(value) {
  return String(value).replace(/\{([^}]+)\}/g, (_, ref) => `var(--${ref.replace(/\./g, '-')})`);
}

/** Flatten a DTCG group into [cssName, cssValue] pairs. */
export function flatten(node, path = []) {
  const out = [];
  for (const [key, value] of Object.entries(node)) {
    if (key.startsWith('$')) continue;
    if (isToken(value)) out.push([[...path, key].join('-'), resolveAliases(value.$value)]);
    else if (value && typeof value === 'object') out.push(...flatten(value, [...path, key]));
  }
  return out;
}

/** Token name → the colour a contrast check should measure (`$extensions.contrast` wins). */
export function measurableColors(group) {
  const out = {};
  for (const [name, token] of Object.entries(group ?? {})) {
    if (name.startsWith('$') || !isToken(token)) continue;
    const value = token.$extensions?.contrast ?? token.$value;
    const color = parseColor(value);
    if (color) out[name] = color;
  }
  return out;
}

/** Parses #rgb, #rrggbb, #rrggbbaa, rgb()/rgba() into {r,g,b,a} (0-255, alpha 0-1). */
export function parseColor(value) {
  const v = String(value).trim();
  let m = /^#([0-9a-f]{3,8})$/i.exec(v);
  if (m) {
    let h = m[1];
    if (h.length === 3 || h.length === 4) h = [...h].map((c) => c + c).join('');
    if (h.length !== 6 && h.length !== 8) return null;
    return {
      r: parseInt(h.slice(0, 2), 16),
      g: parseInt(h.slice(2, 4), 16),
      b: parseInt(h.slice(4, 6), 16),
      a: h.length === 8 ? parseInt(h.slice(6, 8), 16) / 255 : 1,
    };
  }
  m = /^rgba?\(([^)]+)\)$/i.exec(v);
  if (m) {
    const parts = m[1]
      .split(/[\s,/]+/)
      .filter(Boolean)
      .map(Number);
    if (parts.length < 3 || parts.some(Number.isNaN)) return null;
    return { r: parts[0], g: parts[1], b: parts[2], a: parts.length > 3 ? parts[3] : 1 };
  }
  return null;
}

/** Composite a possibly translucent colour over an opaque backdrop. */
export function over(fg, bg) {
  const a = fg.a;
  return {
    r: fg.r * a + bg.r * (1 - a),
    g: fg.g * a + bg.g * (1 - a),
    b: fg.b * a + bg.b * (1 - a),
    a: 1,
  };
}

function channel(c) {
  const s = c / 255;
  return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
}

export function luminance({ r, g, b }) {
  return 0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b);
}

export function contrast(fg, bg) {
  const [a, b] = [luminance(fg), luminance(bg)].sort((x, y) => y - x);
  return (a + 0.05) / (b + 0.05);
}

/** Hue in degrees, 0-360. */
export function hue({ r, g, b }) {
  const [R, G, B] = [r / 255, g / 255, b / 255];
  const max = Math.max(R, G, B);
  const min = Math.min(R, G, B);
  const d = max - min;
  if (d === 0) return 0;
  let h;
  if (max === R) h = ((G - B) / d) % 6;
  else if (max === G) h = (B - R) / d + 2;
  else h = (R - G) / d + 4;
  return (((h * 60) % 360) + 360) % 360;
}

export function hueSeparation(a, b) {
  const d = Math.abs(hue(a) - hue(b)) % 360;
  return d > 180 ? 360 - d : d;
}

/** Simulate the three common colour-vision deficiencies (Brettel-style linear approximation). */
export function simulate(color, type) {
  const m = {
    protanopia: [0.567, 0.433, 0, 0.558, 0.442, 0, 0, 0.242, 0.758],
    deuteranopia: [0.625, 0.375, 0, 0.7, 0.3, 0, 0, 0.3, 0.7],
    tritanopia: [0.95, 0.05, 0, 0, 0.433, 0.567, 0, 0.475, 0.525],
  }[type];
  const { r, g, b } = color;
  return {
    r: m[0] * r + m[1] * g + m[2] * b,
    g: m[3] * r + m[4] * g + m[5] * b,
    b: m[6] * r + m[7] * g + m[8] * b,
    a: 1,
  };
}

/** Perceptual distance, good enough to say "these two are still telling apart". */
export function distance(a, b) {
  return Math.hypot(a.r - b.r, a.g - b.g, a.b - b.b);
}
