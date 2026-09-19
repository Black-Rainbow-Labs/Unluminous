#!/usr/bin/env node
// Builds the SQLite database every picture in `documentation/database.md` is taken of.
//
//   node --no-warnings tools/documentation/library-db.mjs <path>
//
// A small library: two tables with a foreign key between them, a view over the join, and enough
// rows that a grid has something to page through and a blame-like spread of years to look at.
//
// `node:sqlite` rather than the `sqlite3` program, because Node is already what this repository's
// other tools are written in — the changelog, the contrast check, the window-suite receipt and the
// open-source publish are all `.mjs` — and `sqlite3` is on this machine by accident of two other
// installations rather than because anything here asked for it. It is behind `--no-warnings`
// because Node prints one about the module being experimental, and `fixture.ps1` stops on anything
// a command writes to its error output.
//
// Unluminous itself cannot make this file: `rusqlite`'s default flags include one that creates a
// database that is not there, and the Database plugin deliberately turns it off, because a mistyped
// path used to make an empty database and show a data source with nothing in it.

import { DatabaseSync } from 'node:sqlite';
import { existsSync, unlinkSync, mkdirSync } from 'node:fs';
import { dirname } from 'node:path';

const at = process.argv[2];
if (!at) {
  console.error('usage: node --no-warnings tools/documentation/library-db.mjs <path>');
  process.exit(2);
}

mkdirSync(dirname(at), { recursive: true });
// Written fresh, so a picture is never of a database an earlier run had already edited.
for (const suffix of ['', '-wal', '-shm']) if (existsSync(at + suffix)) unlinkSync(at + suffix);

const db = new DatabaseSync(at);

db.exec(`
  CREATE TABLE artist (
      id       INTEGER PRIMARY KEY,
      name     TEXT NOT NULL,
      country  TEXT
  );

  CREATE TABLE album (
      id         INTEGER PRIMARY KEY,
      title      TEXT NOT NULL,
      artist_id  INTEGER NOT NULL REFERENCES artist(id),
      year       INTEGER,
      label      TEXT,
      tracks     INTEGER,
      note       TEXT
  );

  CREATE VIEW recent_albums AS
      SELECT album.title, artist.name AS artist, album.year, album.label
        FROM album
        JOIN artist ON artist.id = album.artist_id
       WHERE album.year >= 2000
       ORDER BY album.year DESC;
`);

const artists = [
  [1, 'Kioko Arai', 'Japan'],
  [2, 'The Long Winter', 'Iceland'],
  [3, 'Marisol Vega', 'Chile'],
  [4, 'Aurora Field', 'Scotland'],
];

const albums = [
  [1, 'Paper Lantern', 1, 2014, 'Hinoki', 11, null],
  [2, 'Nine Bridges', 1, 2019, 'Hinoki', 9, 'remastered'],
  [3, 'Slow Thaw', 2, 2003, 'Northlight', 8, null],
  [4, 'The Long Winter', 2, 2008, 'Northlight', 10, null],
  [5, 'Cordillera', 3, 2016, 'Sur', 12, null],
  [6, 'Salt and Copper', 3, 2021, 'Sur', 7, 'live'],
  [7, 'Beacon', 4, 1998, 'Harbour', 10, null],
  [8, 'Northern Line', 4, 2011, 'Harbour', 13, null],
  [9, 'Green Flash', 4, 2023, 'Harbour', 9, null],
];

const artist = db.prepare('INSERT INTO artist (id, name, country) VALUES (?, ?, ?)');
for (const row of artists) artist.run(...row);

const album = db.prepare(
  'INSERT INTO album (id, title, artist_id, year, label, tracks, note) VALUES (?, ?, ?, ?, ?, ?, ?)',
);
for (const row of albums) album.run(...row);

db.close();
console.log(`built ${at}: ${artists.length} artists, ${albums.length} albums, one view`);
