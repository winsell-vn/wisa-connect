#!/usr/bin/env node
// Publishes a release to the R2 bucket behind assets.winsell.vn, which is the
// host the Tauri updater in src-tauri/tauri.conf.json polls:
//
//   https://assets.winsell.vn/wisa-connect/latest.json
//
// Layout written to the bucket:
//
//   wisa-connect/latest.json          updater manifest
//   wisa-connect/<tag>/<file>         installers, portable zips and .sig files
//
// The manifest is rewritten from the GitHub asset URLs to the R2 mirror with
// scripts/rewrite-updater-manifest.mjs. Minisign signatures cover file contents
// and not URLs, so the updater keeps verifying against the pubkey baked into
// the app.
//
// Usage:
//   node scripts/publish-r2-release.mjs <artifacts-dir> <tag> [options]
//
// Options:
//   --base-url <url>     Public base URL (default: https://assets.winsell.vn/wisa-connect)
//   --bucket <name>      R2 bucket name (default: $R2_BUCKET)
//   --dry-run            Print the planned uploads without writing anything
//
// Requires the wrangler CLI and CLOUDFLARE_API_TOKEN / CLOUDFLARE_ACCOUNT_ID
// (or a prior `wrangler login`). Nothing is uploaded unless every local
// precondition passes, so a half-published release cannot happen.

import { execFileSync } from 'node:child_process';
import { existsSync, mkdtempSync, readdirSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { basename, join } from 'node:path';

const USAGE =
  'Usage: node scripts/publish-r2-release.mjs <artifacts-dir> <tag> [--base-url <url>] [--bucket <name>] [--dry-run]';

const args = process.argv.slice(2);
if (args.length < 2) {
  console.error(USAGE);
  process.exit(1);
}

const [artifactsDir, tag] = args;

// Flags are parsed by hand so an unexpected flag fails loudly instead of being
// silently ignored and publishing the wrong thing.
const flags = { baseUrl: 'https://assets.winsell.vn/wisa-connect', bucket: process.env.R2_BUCKET, dryRun: false };
for (let i = 2; i < args.length; i += 1) {
  const arg = args[i];
  if (arg === '--dry-run') {
    flags.dryRun = true;
  } else if (arg === '--base-url' || arg === '--bucket') {
    const value = args[i + 1];
    if (!value) {
      console.error(`${arg} requires a value\n${USAGE}`);
      process.exit(1);
    }
    if (arg === '--base-url') flags.baseUrl = value;
    else flags.bucket = value;
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
if (!flags.bucket) {
  console.error('No bucket: pass --bucket or set R2_BUCKET');
  process.exit(1);
}
if (!existsSync(artifactsDir)) {
  console.error(`Artifacts directory not found: ${artifactsDir}`);
  process.exit(1);
}

// The updater manifest is the one file the client polls, so publishing without
// it would leave every installed app on the old version. Require it up front.
const manifestName = 'latest.json';
const manifestPath = join(artifactsDir, manifestName);
if (!existsSync(manifestPath)) {
  console.error(`Missing ${manifestName} in ${artifactsDir} — refusing to publish a release no client can find.`);
  process.exit(1);
}

const normalizedBase = flags.baseUrl.replace(/\/+$/, '');
// Bucket keys are the base URL path: https://assets.winsell.vn/wisa-connect
// lives under the wisa-connect/ prefix.
const keyPrefix = new URL(normalizedBase).pathname.replace(/^\/+|\/+$/g, '');

const artifactNames = readdirSync(artifactsDir)
  .filter((name) => name !== manifestName && !name.startsWith('.'))
  .sort();
if (artifactNames.length === 0) {
  console.error(`No artifacts to publish in ${artifactsDir}`);
  process.exit(1);
}

// Fail before touching the network if the manifest points at a file that is not
// here or has no signature beside it. The updater downloads exactly these
// entries, so a missing file or signature would strand every installed client
// on the old version with no clear error.
const manifest = JSON.parse(readFileSync(manifestPath, 'utf8'));
const platformEntries = Object.entries(manifest.platforms ?? {});
if (platformEntries.length === 0) {
  console.error(`${manifestName} has no platforms — the build did not produce updater artifacts.`);
  process.exit(1);
}
const missing = [];
for (const [platform, entry] of platformEntries) {
  if (typeof entry?.url !== 'string' || typeof entry?.signature !== 'string' || entry.signature.trim() === '') {
    console.error(`Platform ${platform} is missing url or signature in ${manifestName}`);
    process.exit(1);
  }
  const fileName = decodeURIComponent(entry.url.split('/').pop());
  if (!artifactNames.includes(fileName)) {
    missing.push(`${fileName} (${platform})`);
  } else if (!artifactNames.includes(`${fileName}.sig`)) {
    missing.push(`${fileName}.sig (${platform})`);
  }
}
if (missing.length > 0) {
  console.error(
    `Manifest references files that are missing from ${artifactsDir}:\n  ${missing.join('\n  ')}\n` +
      'Build with TAURI_SIGNING_PRIVATE_KEY set so every updater artifact is signed.',
  );
  process.exit(1);
}

const uploads = [];
const tmp = mkdtempSync(join(tmpdir(), 'wisa-r2-'));
try {
  // Rewrite the manifest so download URLs point at the R2 mirror. The rewriter
  // validates the GitHub layout, so a surprise in the manifest fails here.
  const rewrittenManifest = join(tmp, manifestName);
  execFileSync(
    process.execPath,
    [join('scripts', 'rewrite-updater-manifest.mjs'), manifestPath, tag, normalizedBase, rewrittenManifest],
    { stdio: 'inherit' },
  );

  for (const name of artifactNames) {
    uploads.push({
      file: join(artifactsDir, name),
      key: `${keyPrefix}/${tag}/${name}`,
      contentType: name.endsWith('.sig') ? 'text/plain' : 'application/octet-stream',
    });
  }
  uploads.push({
    file: rewrittenManifest,
    key: `${keyPrefix}/${manifestName}`,
    contentType: 'application/json',
    // Clients poll this path; a cached copy would stall auto-update.
    cacheControl: 'no-cache, max-age=0',
  });

  console.log(`Publishing ${tag} to r2://${flags.bucket} (${uploads.length} objects)`);
  for (const upload of uploads) {
    console.log(`  ${basename(upload.file)} -> ${upload.key}`);
  }

  if (flags.dryRun) {
    console.log('Dry run: nothing uploaded.');
  } else {
    for (const upload of uploads) {
      const cliArgs = [
        'r2',
        'object',
        'put',
        `${flags.bucket}/${upload.key}`,
        '--file',
        upload.file,
        '--content-type',
        upload.contentType,
        '--remote',
      ];
      if (upload.cacheControl) {
        cliArgs.push('--cache-control', upload.cacheControl);
      }
      execFileSync('wrangler', cliArgs, { stdio: 'inherit', shell: process.platform === 'win32' });
      console.log(`uploaded ${upload.key}`);
    }
    console.log(`Done. Updater endpoint: ${normalizedBase}/${manifestName}`);
  }
} finally {
  rmSync(tmp, { recursive: true, force: true });
}
