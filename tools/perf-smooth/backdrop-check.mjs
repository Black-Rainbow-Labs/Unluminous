#!/usr/bin/env node
// Whether a modal's backdrop holds the window blurred, read from the pixels of a screenshot (`task-2288`).
//
//   node tools/perf-smooth/backdrop-check.mjs --bin <folder with unluminous.exe> --out <folder>
//
// Opens a project with an editor full of text, takes a screenshot without a modal and one a moment after
// Settings opens, and compares a strip of the window outside the dialog in the two. Prints one JSON line:
// the mean brightness of the strip in each, and how much neighbouring pixels differ in each, which is what text
// looks like to a number. A backdrop that is only the dark scrim has almost no difference between neighbours
// and no trace of the window's own brightness; a sharp photograph keeps the window's differences; a blur keeps
// its brightness and loses most of its differences.

import { execFileSync } from 'node:child_process';
import { copyFileSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import zlib from 'node:zlib';

const args = process.argv.slice(2);
/** Read a `--name value` argument, or the fallback. */
const option = (name, fallback) => {
  const at = args.indexOf(`--${name}`);
  return at >= 0 ? args[at + 1] : fallback;
};
const bin = path.resolve(option('bin', 'target/release'));
const out = path.resolve(option('out', '_agent_output/perf-smooth/backdrop'));
const here = path.dirname(new URL(import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, '$1'));
const repo = path.resolve(here, '..', '..');
const home = path.join(os.tmpdir(), 'unluminous-backdrop-check', path.basename(out));
const project = path.join(home, 'project');
const appdata = path.join(home, 'appdata');
rmSync(home, { recursive: true, force: true });
mkdirSync(path.join(project, 'src'), { recursive: true });
mkdirSync(path.join(appdata, 'Unluminous'), { recursive: true });
mkdirSync(out, { recursive: true });
copyFileSync(path.join(repo, 'crates/unluminous-app/src/app/mod.rs'), path.join(project, 'src/app.rs'));
writeFileSync(path.join(appdata, 'Unluminous', 'settings.conf'), 'telemetry.enabled = false\nupdate.check = never\neditor.servers = off\n');
const sleep = (ms) => Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, ms);

const pid = Number(execFileSync('pwsh', ['-NoProfile', '-Command',
  `$env:APPDATA='${appdata}'; $env:UNLUMINOUS_SESSION='off'; $env:UNLUMINOUS_FRAME_TRACE='${path.join(out, 'trace.txt')}'; ` +
  `(Start-Process -FilePath '${path.join(bin, 'unluminous.exe')}' -ArgumentList @('--background','${project}') -PassThru).Id`,
], { encoding: 'utf8' }).trim());
/** One command against the window. */
const cli = (...words) => execFileSync(path.join(bin, 'unluminous-cli.exe'), ['--instance', String(pid), '--timeout', '30000', ...words], {
  encoding: 'utf8', env: { ...process.env, APPDATA: appdata }, stdio: ['ignore', 'pipe', 'ignore'],
});
for (let attempt = 0; ; attempt++) {
  try { cli('status', '--quiet'); break; } catch { if (attempt > 200) throw new Error('the window never answered'); sleep(100); }
}

/** Read an 8 bit RGBA or RGB PNG, as written by `window screenshot`, into its size and pixels. */
function readPng(file) {
  const bytes = readFileSync(file);
  let at = 8;
  let width = 0, height = 0, channels = 4;
  const data = [];
  while (at < bytes.length) {
    const length = bytes.readUInt32BE(at);
    const type = bytes.toString('ascii', at + 4, at + 8);
    const body = bytes.subarray(at + 8, at + 8 + length);
    if (type === 'IHDR') {
      width = body.readUInt32BE(0);
      height = body.readUInt32BE(4);
      channels = body[9] === 6 ? 4 : 3;
    } else if (type === 'IDAT') data.push(body);
    at += 12 + length;
  }
  const raw = zlib.inflateSync(Buffer.concat(data));
  const stride = width * channels;
  const pixels = Buffer.alloc(height * stride);
  for (let y = 0; y < height; y++) {
    const filter = raw[y * (stride + 1)];
    for (let x = 0; x < stride; x++) {
      const value = raw[y * (stride + 1) + 1 + x];
      const left = x >= channels ? pixels[y * stride + x - channels] : 0;
      const up = y > 0 ? pixels[(y - 1) * stride + x] : 0;
      const corner = x >= channels && y > 0 ? pixels[(y - 1) * stride + x - channels] : 0;
      let predicted = 0;
      if (filter === 1) predicted = left;
      else if (filter === 2) predicted = up;
      else if (filter === 3) predicted = (left + up) >> 1;
      else if (filter === 4) {
        const p = left + up - corner;
        const [a, b, c] = [Math.abs(p - left), Math.abs(p - up), Math.abs(p - corner)];
        predicted = a <= b && a <= c ? left : b <= c ? up : corner;
      }
      pixels[y * stride + x] = (value + predicted) & 255;
    }
  }
  return { width, height, channels, pixels };
}

/** The mean brightness of a strip at the left of the picture, and the mean difference between neighbours in it. */
function strip(picture) {
  const { width, height, channels, pixels } = picture;
  const x0 = Math.floor(width * 0.02), x1 = Math.floor(width * 0.12);
  const y0 = Math.floor(height * 0.2), y1 = Math.floor(height * 0.8);
  let brightness = 0, difference = 0, count = 0;
  for (let y = y0; y < y1; y++) {
    for (let x = x0; x < x1; x++) {
      const at = (y * width + x) * channels;
      const next = at + channels;
      const value = (pixels[at] + pixels[at + 1] + pixels[at + 2]) / 3;
      brightness += value;
      difference += Math.abs(value - (pixels[next] + pixels[next + 1] + pixels[next + 2]) / 3);
      count++;
    }
  }
  return { brightness: brightness / count, difference: difference / count };
}

try {
  cli('window', 'size', '--width', '1600', '--height', '1000');
  cli('explorer', 'hide');
  cli('tab', 'open', 'src/app.rs');
  sleep(2500);
  const before = path.join(out, 'before.png');
  const during = path.join(out, 'during.png');
  cli('window', 'screenshot', before);
  cli('modal', 'open', 'about');
  sleep(1200);
  cli('window', 'screenshot', during);
  const result = { before: strip(readPng(before)), during: strip(readPng(during)) };
  console.log(JSON.stringify(result));
} finally {
  try { cli('quit'); } catch { /* the kill below is the answer */ }
  sleep(1000);
  try { process.kill(pid); } catch { /* already gone */ }
}
