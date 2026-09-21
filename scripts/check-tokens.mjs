#!/usr/bin/env node
// Scheme completeness, accent hue separation, and the rule that every token value is measured.
import {
  distance,
  hue,
  hueSeparation,
  loadTheme,
  measurableColors,
  pairings,
  primitives,
  simulate,
} from './tokens-lib.mjs';

const schemes = ['light', 'dark'];
const failures = [];
const theme = loadTheme();

for (const scheme of schemes) {
  const group = theme[scheme];
  if (!group) {
    failures.push(`missing the "${scheme}" scheme entirely`);
    continue;
  }
  for (const name of primitives.$semantics) {
    if (!(name in group)) failures.push(`${scheme}: missing semantic token --${name}`);
  }
  for (const name of Object.keys(group)) {
    if (!name.startsWith('$') && !primitives.$semantics.includes(name)) {
      failures.push(`${scheme}: --${name} is not a declared semantic role`);
    }
  }

  // A value that is neither measured nor a recorded deviation is a value someone invented.
  for (const [name, token] of Object.entries(group)) {
    if (name.startsWith('$')) continue;
    const measured = /^measured:/.test(token.$description ?? '');
    const deviation = token.$extensions?.sanctum?.deviation;
    if (!measured && !deviation) {
      failures.push(`${scheme}: --${name} records neither a measurement nor a deviation`);
    }
  }

  // Accent must not read as a status colour.
  const colors = measurableColors(group);
  const rule = pairings.hueSeparation;
  const accent = colors[rule.accent];
  if (!accent) {
    failures.push(`${scheme}: accent has no measurable colour`);
    continue;
  }
  for (const other of rule.against) {
    if (!colors[other]) continue;
    const sep = hueSeparation(accent, colors[other]);
    if (sep < rule.minDegrees) {
      failures.push(
        `${scheme}: accent (${hue(accent).toFixed(0)}°) is only ${sep.toFixed(0)}° from --${other} (${hue(colors[other]).toFixed(0)}°), needs ${rule.minDegrees}°`,
      );
    }
  }
  for (const other of rule.distinguishFrom) {
    if (!colors[other]) continue;
    for (const cvd of ['protanopia', 'deuteranopia', 'tritanopia']) {
      const d = distance(simulate(accent, cvd), simulate(colors[other], cvd));
      if (d < 40) {
        failures.push(
          `${scheme}: accent is indistinguishable from --${other} under ${cvd} (distance ${d.toFixed(0)}, needs 40)`,
        );
      }
    }
  }
}

if (failures.length > 0) {
  console.error(`Token validation failed (${failures.length}):`);
  for (const f of failures) console.error(`  ${f}`);
  process.exit(1);
}
console.log(`Token validation passed for ${theme.$id} × ${schemes.length} schemes.`);
