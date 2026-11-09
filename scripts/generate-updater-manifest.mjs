#!/usr/bin/env node
// Builds the Tauri updater manifest (latest.json) from a directory of build
// artifacts. Tauri writes a .sig beside every updater artifact but does not
// write latest.json itself, so the release pipeline has to assemble it.
//
// Usage:
//   node scripts/generate-updater-manifest.mjs <artifacts-dir> <tag> [options]
//
// Options:
//   --base-url <url>   Public base URL the artifacts will be served from.
//                      Default: the GitHub release download URL for <tag>.
//   --notes <text>     Release notes copied into the manifest.
//   --output <path>    Default: <artifacts-dir>/latest.json
//
// Anything that cannot be classified fails the build instead of being skipped:
// a manifest that silently omits a platform leaves those users stuck on the old
// version, which is worse than a red build.

import { existsSync, readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

const USAGE =
  'Usage: node scripts/generate-updater-manifest.mjs <artifacts-dir> <tag> [--base-url <url>] [--notes <text>] [--output <path>]';

const args = process.argv.slice(2);
if (args.length < 2) {
  console.error(USAGE);
  process.exit(1);
}

const [artifactsDir, tag] = args;

const flags = { baseUrl: undefined, notes: '', output: undefined };
for (let i = 2; i < args.length; i += 1) {
  const arg = args[i];
  if (arg === '--base-url' || arg === '--notes' || arg === '--output') {
    const value = args[i + 1];
    if (value === undefined) {
      console.error(`${arg} requires a value\n${USAGE}`);
      process.exit(1);
    }
    if (arg === '--base-url') flags.baseUrl = value;
    else if (arg === '--notes') flags.notes = value;
    else flags.output = value;
    i += 1;
  } else {
    console.error(`Unknown argument: ${arg}\n${USAGE}`);
    process.exit(1);
  }
}

if (!/^v\d+\.\d+\.\d+/.test(tag)) {
  console.error(`Tag must look like v1.2.3, got: ${tag}`);
  process.exit(1);
}
if (!existsSync(artifactsDir)) {
  console.error(`Artifacts directory not found: ${artifactsDir}`);
  process.exit(1);
}

const version = tag.replace(/^v/, '');
const baseUrl = (
  flags.baseUrl ??
  `https://github.com/winsell-vn/wisa-connect/releases/download/${tag}`
).replace(/\/+$/, '');

// Tauri names updater artifacts after the product and version, e.g.
//   WISA Connect_0.1.0_x64-setup.nsis.zip        (Windows NSIS)
//   WISA Connect_0.1.0_x64_en-US.msi.zip         (Windows MSI)
//   WISA Connect_0.1.0_aarch64.app.tar.gz        (macOS)
//   WISA Connect_0.1.0_amd64.AppImage.tar.gz     (Linux)
// Longest suffix first so aarch64 is not mistaken for x64.
const RULES = [
  { test: /_aarch64\.app\.tar\.gz$/, platform: 'darwin-aarch64' },
  { test: /_x64\.app\.tar\.gz$/, platform: 'darwin-x86_64' },
  { test: /_aarch64\.app\.tar\.gz\.sig$/, platform: null },
  { test: /_x64\.app\.tar\.gz\.sig$/, platform: null },
  { test: /_arm64-setup\.nsis\.zip$/, platform: 'windows-aarch64' },
  { test: /_x64-setup\.nsis\.zip$/, platform: 'windows-x86_64' },
  { test: /_arm64_en-US\.msi\.zip$/, platform: 'windows-aarch64' },
  { test: /_x64_en-US\.msi\.zip$/, platform: 'windows-x86_64' },
  { test: /_aarch64\.AppImage\.tar\.gz$/, platform: 'linux-aarch64' },
  { test: /_amd64\.AppImage\.tar\.gz$/, platform: 'linux-x86_64' },
];

const files = readdirSync(artifactsDir).sort();
const platforms = {};
const classified = [];

for (const name of files) {
  if (name === 'latest.json' || name.startsWith('.')) continue;
  const rule = RULES.find((entry) => entry.test.test(name));
  if (!rule || !rule.platform) continue;
  classified.push({ name, platform: rule.platform });
}

if (classified.length === 0) {
  console.error(
    `No updater artifacts found in ${artifactsDir}.\n` +
      'Expected names like "WISA Connect_0.1.0_x64-setup.nsis.zip" or "WISA Connect_0.1.0_aarch64.app.tar.gz".',
  );
  process.exit(1);
}

for (const { name, platform } of classified) {
  const sigPath = join(artifactsDir, `${name}.sig`);
  if (!existsSync(sigPath)) {
    console.error(
      `Missing signature for ${name}. Build with TAURI_SIGNING_PRIVATE_KEY set so Tauri writes the .sig beside each updater artifact.`,
    );
    process.exit(1);
  }
  const signature = readFileSync(sigPath, 'utf8').trim();
  if (signature === '') {
    console.error(`Empty signature in ${name}.sig`);
    process.exit(1);
  }
  platforms[platform] = {
    signature,
    url: `${baseUrl}/${encodeURIComponent(name)}`,
  };
}

const manifest = {
  version,
  ...(flags.notes ? { notes: flags.notes } : {}),
  pub_date: new Date().toISOString(),
  platforms,
};

const output = flags.output ?? join(artifactsDir, 'latest.json');
writeFileSync(output, `${JSON.stringify(manifest, null, 2)}\n`);
console.log(`Wrote ${output} with ${Object.keys(platforms).length} platform(s): ${Object.keys(platforms).join(', ')}`);
