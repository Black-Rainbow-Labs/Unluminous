Settings screen 9 of 9: Plugins › Database
Drawn by crates/unluminous-app/src/components/database/settings_page.rs. Stored in plugins/database/sources.conf (keys below are lines in that file). The page scrolls.

**Data sources**
Help: "Where each one points, and where its password is kept. Unluminous never writes a password into a file: it goes into this machine's own credential store — the keychain on macOS, the Secret Service on Linux, Credential Manager on Windows — and what is written down is the name of the entry."
A list, two lines per data source:
- Line 1: the name in strong text, an Edit button (60 wide, opens the Data Source dialog) and a Remove button (78 wide, removes it at once).
- Line 2, faint: "<where it points> · <connected | connected, read only | read only | not connected> · password <none | environment VAR | keychain … | typed, until this window closes>".
Empty list: "There are none yet."
Under the list: "New data source" button (150 wide), opens the dialog.
Stored per source N: source.N.name, source.N.engine, then source.N.file (SQLite, Inillucent) or source.N.host, .port, .database, .user, .sslmode (PostgreSQL), plus source.N.password.env or source.N.password.keychain.

**Rows**
- (number field, 90 wide) followed by the words "rows a page, in a grid and in a console result" | 1 to 100,000 | default 200 | page_size
Help: "One more than this is asked for and thrown away, which is what makes `1-200 of 200+` honest: nobody counted the rest."

**The Data Source dialog** (separate modal, 520 by 470, titled "New Data Source" or "Data Source"; components/database/modal.rs)
Section "General" with a "Test Connection" button and its result. Fields:
- Name | text field | every engine
- Engine | three choice buttons, 90 wide | PostgreSQL, SQLite, Inillucent | default PostgreSQL
- File | text field and a "Browse…" button | SQLite, Inillucent. Help: "The path of a SQLite file. It is opened, never created - a mistyped path says so rather than quietly making an empty database."
- Host and port | two text fields | PostgreSQL | localhost, 5432
- Database | text field | PostgreSQL
- User | text field | PostgreSQL
- Password | password field | PostgreSQL. Help depends on where the password is: none yet: "Kept in this machine's own credential store … Unluminous writes no password to a file, ever: what the settings file records is the name of the entry." / already stored: "There is a password kept for this data source in this machine's own credential store. Type a new one to replace it, or leave this empty to keep the one that is there." / from an environment variable: "This data source reads its password from an environment variable, which is how it was set up. Type one here to move it into this machine's own credential store instead."
- Connection security | three choice buttons | Off, If offered, Required | default If offered | PostgreSQL. Help: "Whether the connection to the server is encrypted. Required refuses to connect without it. This is about the connection, not about how your password is kept - that is always in this machine's own credential store."
Footer: Cancel and Save.

**Not built yet**
- A source can be read only (shown on line 2) but nothing here sets it; only `unluminous-cli plugins run database read-only`.
- Which source is chosen (chosen) is picked in the Database pane, not here.
