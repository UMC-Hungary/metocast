#!/usr/bin/env node
// Every exported component carries a specification and stories, and every specification and
// story belongs to something exported. Undocumented work is the state the previous attempt
// shipped in; this is what stops it recurring.
import { existsSync, readFileSync, readdirSync } from 'node:fs';
import { join } from 'node:path';
import { packageDir } from './tokens-lib.mjs';

const entry = join(packageDir, 'src/index.ts');
const docsDir = join(packageDir, 'docs');
const storiesDir = join(packageDir, 'stories');

/** Component exports only — actions, stores and types are not catalog entries. */
const components = [
  ...readFileSync(entry, 'utf8').matchAll(/^export \{ default as (\w+) \}/gm),
].map((m) => m[1]);

const REQUIRED_SECTIONS = [
  'Anatomy',
  'Props',
  'Variants',
  'States',
  'Tokens consumed',
  'Keyboard',
  'ARIA',
  'Accessibility acceptance criteria',
  'Reference correspondence',
  'Recorded deviations',
];

const failures = [];

for (const name of components) {
  const doc = join(docsDir, `${name}.md`);
  const story = join(storiesDir, `${name}.stories.ts`);
  if (!existsSync(doc)) failures.push(`${name}: no specification at docs/${name}.md`);
  else {
    const text = readFileSync(doc, 'utf8');
    const missing = REQUIRED_SECTIONS.filter((section) => !text.includes(`## ${section}`));
    if (missing.length > 0) {
      failures.push(
        `${name}: specification is missing the section${missing.length > 1 ? 's' : ''} ${missing.join(', ')}`,
      );
    }
  }
  if (!existsSync(story)) failures.push(`${name}: no stories at stories/${name}.stories.ts`);
}

const known = new Set(components);
for (const file of readdirSync(docsDir)) {
  const name = file.replace(/\.md$/, '');
  if (file.startsWith('_') || !file.endsWith('.md')) continue;
  if (!known.has(name))
    failures.push(`docs/${file} documents ${name}, which the package does not export`);
}
for (const file of readdirSync(storiesDir)) {
  const name = file.replace(/\.stories\.ts$/, '');
  if (!file.endsWith('.stories.ts')) continue;
  if (!known.has(name))
    failures.push(`stories/${file} covers ${name}, which the package does not export`);
}

if (failures.length > 0) {
  console.error(`Catalog completeness failed (${failures.length}):`);
  for (const failure of failures) console.error(`  ${failure}`);
  process.exit(1);
}
console.log(
  `Catalog complete: ${components.length} components, each with a specification and stories.`,
);
