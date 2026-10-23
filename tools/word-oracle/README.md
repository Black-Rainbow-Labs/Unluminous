# Word oracle

Scripts that ask Microsoft Word what it thinks of a document, so the Rust tests in `crates/unluminous-word` can compare against Word's own answers. They need Word installed and use COM. Each run starts its own hidden Word process and quits only that process.

- `inspect.ps1 -Path <file.docx> -Out <out.json>` opens one document read only and writes a JSON report: statistics, every paragraph (style, list string, indents, spacing, page, positions, character runs), tables, sections, headers and footers, notes, comments, revisions, pictures, fields, bookmarks, hyperlinks and document properties.
- `make-fixtures.ps1 [-OutDir <dir>] [-Only name,name]` builds each fixture from scratch with Word, saves it as `.docx` in `crates/unluminous-word/tests/fixtures/` and runs `inspect.ps1` on it. The author is set to "Oracle" while it runs and restored afterwards. A full run takes several minutes.
- `roundtrip.ps1 -Folder <dir>` runs `inspect.ps1` on every `.docx` in a folder and writes `<name>.word.json` beside each.
- `WordCom.ps1` is shared code that starts and stops the private Word process.

The fixtures and their `.word.json` files are committed, so the Rust tests do not need Word. Run the scripts again only to add or change a fixture.

    pwsh -NoProfile -File tools/word-oracle/make-fixtures.ps1
    pwsh -NoProfile -File tools/word-oracle/roundtrip.ps1 -Folder crates/unluminous-word/tests/fixtures
