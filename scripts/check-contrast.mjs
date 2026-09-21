#!/usr/bin/env node
// Computes WCAG 2.2 contrast for every documented token pairing in every theme × scheme.
// Writes a JSON report for the catalog and exits non-zero on any failure.
import { writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { contrast, loadTheme, measurableColors, over, pairings, tokensDir } from './tokens-lib.mjs';

const thresholds = { text: 4.5, large: 3, ui: 3 };
const report = [];
const theme = loadTheme();

for (const scheme of ['light', 'dark']) {
  const colors = measurableColors(theme[scheme]);
  for (const pairing of pairings.pairings) {
    const fg = colors[pairing.fg];
    const bg = colors[pairing.bg];
    const base = colors[pairing.base ?? 'surface-base'];
    if (!fg || !bg || !base) {
      console.error(
        `  ${scheme}: pairing --${pairing.fg} on --${pairing.bg} has no measurable colour`,
      );
      process.exitCode = 1;
      continue;
    }
    const solidBg = over(bg, base);
    const ratio = contrast(over(fg, solidBg), solidBg);
    const threshold = thresholds[pairing.level];
    report.push({
      scheme,
      fg: pairing.fg,
      bg: pairing.bg,
      level: pairing.level,
      ratio: Math.round(ratio * 100) / 100,
      threshold,
      passes: ratio >= threshold,
    });
  }
}

const outFile = join(tokensDir, 'generated/contrast-report.json');
writeFileSync(outFile, JSON.stringify(report, null, '\t') + '\n');

const failures = report.filter((r) => !r.passes);
if (failures.length > 0) {
  console.error(`Contrast failures (${failures.length} of ${report.length}):`);
  for (const f of failures) {
    console.error(
      `  ${f.scheme}: --${f.fg} on --${f.bg} = ${f.ratio}:1, needs ${f.threshold}:1 (${f.level})`,
    );
  }
  if (!process.argv.includes('--report-only')) process.exit(1);
} else {
  console.log(`Contrast passed for all ${report.length} pairings.`);
}
