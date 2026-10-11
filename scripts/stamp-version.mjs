#!/usr/bin/env node
// Stamps a release version into the files a tag build ships, so the four jobs in build.yml share
// one implementation: node scripts/stamp-version.mjs <version> <file>...
// A .toml file gets every top-level `version = "..."` line replaced; anything else is JSON with a
// "version" key.
import { readFileSync, writeFileSync } from 'node:fs';

const [version, ...files] = process.argv.slice(2);
if (!version || files.length === 0) {
  console.error('usage: stamp-version.mjs <version> <file>...');
  process.exit(1);
}

for (const file of files) {
  const text = readFileSync(file, 'utf8');
  let stamped;
  if (file.endsWith('.toml')) {
    stamped = text.replace(/^version = ".*"/gm, `version = "${version}"`);
  } else {
    const json = JSON.parse(text);
    json.version = version;
    stamped = JSON.stringify(json, null, 2) + '\n';
  }
  writeFileSync(file, stamped);
  console.log(`${file}: ${version}`);
}
