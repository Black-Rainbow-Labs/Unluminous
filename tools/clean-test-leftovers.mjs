// What Unluminous's test suites have left in the temporary folder and in Word's registry records
// (`task-2364`).
//
//   node tools/clean-test-leftovers.mjs            say what there is, and remove nothing
//   node tools/clean-test-leftovers.mjs --delete   remove it
//
// Since `task-2364` a run of `tools/test.mjs` or `tools/window-suite.mjs --run` writes everything into a
// temporary folder of its own and removes it when it ends, and a run that was killed is removed by the
// next one. This is for the rest:
//
//   - **Folders a run left** under `%TEMP%\unluminous-tests` whose process has gone.
//   - **What runs before `task-2364` left at the top of `%TEMP%`**: every `unluminous-*` entry older than a
//     day, except the few the shipped program uses there itself (`KEPT`). A plain `cargo test` typed by
//     hand still writes these, because it has no runner to give it a folder.
//   - **Word's Resiliency records** naming a path in the temporary folder, which a Word stopped while it
//     opened a test's document leaves, and which make the next Word that opens the path wait on a
//     dialog nobody can see.
//
// On 2026-10-25 the top of `%TEMP%` held 169,221 entries: 54,925 `unluminous-*` from this repository's
// tests and 105,562 `inillucent-*` from Inillucent's. Python lists the folder it is started in, and a
// kernel started there gave no answer in 188 s where one started in an empty folder answered in 17 s
// (task-2359). The `inillucent-*` entries are Inillucent's suites' to remove, and this leaves them.

import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { execFileSync } from 'node:child_process';
import { pruneStaleRuns, removeFolder, runsRoot } from './test-runs.mjs';

// Entries at the top of `%TEMP%` the shipped program writes for itself, or that a run in progress needs.
const KEPT = [
  /^unluminous-tests$/,
  /^unluminous-python$/,
  /^unluminous-update-/,
  /^unluminous-report-/,
  /^unluminous-window-tests\.(lock|holder)$/,
];

const OLDER_THAN_MS = 24 * 60 * 60 * 1000;

/** The `unluminous-*` entries at the top of the temporary folder that a test left, older than a day. */
function legacyLeftovers() {
  const temp = process.env.UNLUMINOUS_MACHINE_TEMP || os.tmpdir();
  const found = [];
  for (const name of fs.readdirSync(temp)) {
    if (!name.startsWith('unluminous-') || KEPT.some((rule) => rule.test(name))) continue;
    const full = path.join(temp, name);
    try {
      if (Date.now() - fs.statSync(full).mtimeMs > OLDER_THAN_MS) found.push(full);
    } catch {
      // Gone while it was being looked at.
    }
  }
  return found;
}

/**
 * Word's Resiliency values whose data names a path in the temporary folder, as `key|value` lines.
 * @param remove - whether to remove them as well
 */
function wordRecords(remove) {
  if (process.platform !== 'win32') return [];
  const temp = (process.env.UNLUMINOUS_MACHINE_TEMP || os.tmpdir()).replace(/\\$/, '');
  const script = `
$base = 'HKCU:\\Software\\Microsoft\\Office\\16.0\\Word\\Resiliency'
if (-not (Test-Path $base)) { return }
$needle = '${temp.replaceAll("'", "''")}'.ToLowerInvariant()
foreach ($key in @(Get-ChildItem -Path $base -Recurse -ErrorAction SilentlyContinue)) {
  foreach ($name in @($key.GetValueNames())) {
    $value = $key.GetValue($name)
    $text = if ($value -is [byte[]]) { [Text.Encoding]::Unicode.GetString($value) + [Text.Encoding]::Default.GetString($value) } else { "$value" }
    if ($text.ToLowerInvariant().Contains($needle)) {
      "$($key.Name)|$name"
      if (${remove ? '$true' : '$false'}) { Remove-ItemProperty -LiteralPath $key.PSPath -Name $name -ErrorAction SilentlyContinue -Confirm:$false }
    }
  }
}`;
  try {
    return execFileSync('pwsh', ['-NoProfile', '-NonInteractive', '-Command', script], { encoding: 'utf8' }).split(/\r?\n/).filter(Boolean);
  } catch {
    return [];
  }
}

/** Say what there is, and remove it with `--delete`. */
function main() {
  const remove = process.argv.includes('--delete');
  const runs = fs.existsSync(runsRoot()) ? fs.readdirSync(runsRoot()).length : 0;
  const legacy = legacyLeftovers();
  const records = wordRecords(remove);
  console.log(`Run folders under ${runsRoot()}: ${runs}`);
  console.log(`Entries older than a day that tests left at the top of the temporary folder: ${legacy.length}`);
  console.log(`Word Resiliency records naming the temporary folder: ${records.length}`);
  if (!remove) {
    console.log('\nNothing was removed. Run it again with --delete to remove them.');
    return;
  }
  const pruned = pruneStaleRuns();
  let removed = 0;
  for (const entry of legacy) if (removeFolder(entry)) removed++;
  console.log(`\nRemoved ${pruned} run folders, ${removed} of ${legacy.length} older entries and ${records.length} Word records.`);
}

main();
