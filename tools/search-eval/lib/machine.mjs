// The measurement rules of TDD section 7.6 that are about the machine rather than the queries: both
// arms run on the performance cores, and a run is not graded when the box is busier than it was when
// the quiet reference was taken.

import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { AFFINITY_MASK, QUIET_TOLERANCE, evalRoot } from './config.mjs';

/**
 * Pins this process to the performance cores. Every child it starts afterwards (the rg processes and
 * the index host) inherits the mask, which is how both arms end up on the same cores.
 */
export function pinToPerformanceCores() {
  if (process.platform !== 'win32') return null;
  execFileSync('powershell', ['-NoProfile', '-Command', `(Get-Process -Id ${process.pid}).ProcessorAffinity = ${AFFINITY_MASK}`]);
  const read = execFileSync('powershell', ['-NoProfile', '-Command', `(Get-Process -Id ${process.pid}).ProcessorAffinity`], { encoding: 'utf8' }).trim();
  return Number(read);
}

/** A description of the machine for the manifest. */
export function machine() {
  return { cpu: os.cpus()[0]?.model, logical: os.cpus().length, memoryGb: Math.round(os.totalmem() / 2 ** 30), platform: `${os.platform()} ${os.release()}`, affinityMask: AFFINITY_MASK };
}

const REFERENCE = () => path.join(evalRoot(), 'quiet-reference.json');

/** The quiet reference recorded when the box was quiet, or null before one exists. */
export function readQuietReference() {
  return fs.existsSync(REFERENCE()) ? JSON.parse(fs.readFileSync(REFERENCE(), 'utf8')) : null;
}

/**
 * Records the quiet reference: the median time of the rg arm on the reference queries. Only done in
 * a granted quiet window.
 * @param median - the median milliseconds
 * @param detail - what it was measured on
 */
export function writeQuietReference(median, detail) {
  fs.writeFileSync(REFERENCE(), JSON.stringify({ median, detail, at: new Date().toISOString() }, null, 2));
}

/**
 * Whether the rg arm's reference queries ran more than 3% slower than the quiet reference, in which
 * case the run is not graded (exit 4).
 * @param median - this run's median on the reference queries
 */
export function busierThanReference(median) {
  const reference = readQuietReference();
  if (!reference) return { busier: false, reference: null, ratio: null };
  const ratio = median / reference.median;
  return { busier: ratio > 1 + QUIET_TOLERANCE, reference: reference.median, ratio };
}
