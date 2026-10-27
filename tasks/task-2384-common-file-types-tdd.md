# task-2384: plugins for the file types engineers open every day

> "Let's create plugins for additional file types that are common for engineers. dockerfile, sh, bash,
> yaml, Env, json, etc. Research what we have, common ones that IntelliJ supports, create a tdd, then
> fully implement."

## 1. What Unluminous had before this ticket

Eleven language plugins shipped: Rust, JavaScript, TypeScript, CSS, HTML, Mermaid, SQL, Python, JSON,
TOML, YAML and Shell (`.sh`, `.bash`, `.zsh`). Every one of them claimed files by extension only:
`Plugin::claims`, `Grammars::for_path` and `Plugins::for_path` all read `Path::extension`.

Three findings shaped the work.

**A file named for what it is had no plugin and could not have one.** Rust reads `Dockerfile`,
`Makefile`, `.env`, `.gitignore`, `.bashrc` and `.editorconfig` as having no extension at all, and
`Dockerfile.dev` and `.env.local` as having the extensions `dev` and `local`. So none of the files
the ticket names first could be claimed by a manifest, whatever it said. `file_kind::kind_name`
named `Makefile` and `Dockerfile` by hand, so the status bar knew what they were while nothing could
colour them, and a `.gitignore` was "Plain text" and was offered Bold and Italic.

**Twelve programming languages were read by the code index and coloured by nothing.** The code index
(atrius-index 0.3.2) bundles manifests for C, C++, C#, Go, Java, Kotlin, Lua, PHP, Ruby, Scala, Swift
and Zig. `Plugins::settle` gives those grammars to any extension no plugin claims, so a `.go` file had
Go to Definition and completion and was drawn entirely in the text colour, because colouring needs a
plugin: `colour_the_file` returns early when `Plugins::for_path` answers with nothing.

**A configuration file is a list of names and values, and the tokeniser could not see a name.** The
YAML plugin's own manifest says so: "a YAML document is almost entirely bare, unquoted words, and the
tokeniser has no way to tell a mapping key from a plain scalar value". TOML said the same about its
section headers. A `.env`, INI or properties plugin written with the old rules would colour a comment
and a quoted value and nothing else.

## 2. What IntelliJ supports

IntelliJ IDEA Ultimate ships file types, some built in and some as bundled plugins, for each of the
following. The column on the right is what Unluminous had.

| File type | IntelliJ | Unluminous before |
|---|---|---|
| Dockerfile | Docker plugin, bundled | named in the status bar, not coloured |
| `.env` | `.env files support` plugin | "Settings", not coloured |
| Shell (`.sh`, `.bash`, `.zsh`, start up files) | Shell Script plugin, bundled | extensions only |
| YAML | built in | coloured, keys not told from values |
| JSON, JSON5, JSON Lines | built in | JSON only; `.jsonc` and `tsconfig.json` comments read as errors |
| TOML | TOML plugin, bundled | coloured, section headers by their brackets only |
| Properties | built in | not coloured |
| INI, `.editorconfig`, `.gitconfig` | Ini and EditorConfig plugins | not coloured |
| XML and its project files | built in | not coloured |
| Makefile | Makefile Language plugin | named in the status bar, not coloured |
| CMake | CMake support (CLion) | not coloured |
| `.gitignore` and other ignore files | .ignore plugin | plain text, offered formatting |
| PowerShell | PowerShell plugin | named "Batch", not coloured |
| Batch | Batch Scripts Support plugin | not coloured |
| Terraform and HCL | Terraform and HCL plugin, bundled | not coloured |
| Protocol Buffers | Protocol Buffers plugin, bundled | not coloured |
| GraphQL | GraphQL plugin | not coloured |
| Groovy and Gradle | built in | not coloured |
| Go, Kotlin, C, C++, C#, Ruby, PHP, Lua, Swift, Scala, Zig, Java | built in or language plugins | read by the index, not coloured |

## 3. Design

### 3.1 Claiming a file by its name: `language.file_names`

A comma list of whole file names, each of which may hold one `*` for any run of characters:

```
language.file_names = Dockerfile, Dockerfile.*, Containerfile, Containerfile.*
```

Rules:

- Matched against the file's whole name without regard to case, because Windows and macOS file
  systems ignore case and a `dockerfile` is a Dockerfile.
- **A whole name beats an extension**, inside a plugin and across plugins. `Plugins::for_path` asks
  every switched on plugin for a name match before it asks any for an extension match, and `Grammars`
  keeps a second list, `by_name`, asked in the same order. That is what makes `tsconfig.json` JSON
  with comments while `package.json` stays JSON. A pattern with a `*` that names its own extension,
  `tsconfig.*.json`, counts as a whole name.
- **A name whose `*` stands for the extension is asked last** (`types::ends_open`). `Dockerfile.*`
  says nothing about what the extension makes the file, so it is asked after every extension claim
  and never for a file Unluminous opens its own way (`file_kind::opens_its_own_way`: pictures,
  Markdown, notebooks, plain text, tables, Word documents, workbooks, PDFs, diagrams, realms). So
  `Dockerfile.dev` is a Dockerfile, `Dockerfile.yml` is YAML and `Dockerfile.md` keeps its Markdown
  preview. The first version asked these first too, and the test that found it was the one that
  opened `Dockerfile.md`.
- One `*` and no folder separator. A manifest that breaks either is refused with a sentence, which is
  the rule every checked key keeps. A folder in a pattern (`.vscode/settings.json`) was weighed and
  left out: every caller has a path, but a pattern language with folders is a glob syntax, and the
  bundled plugins need none.
- A language plugin may claim names and no extension. The ignore plugin does.

The key is read by `services::plugins::manifest` and kept on `Plugin::file_names`; it is not part of
`unluminous_core::Grammar`, because the core crate never decides which file a grammar is for.

### 3.2 Three tokeniser rules for configuration files

Each is a manifest key, off unless a language names it, so every plugin that shipped before reads
exactly as it did. That is the rule every key since `task-1671` has kept, and
`a_grammar_that_names_none_of_the_new_keys_reads_exactly_as_before` checks it.

**`language.keys`**, a list of delimiter characters such as `=, :`. A run of characters at the head of
a line that is followed (after spaces) by a delimiter is a key, drawn as `Token::Type`. The head of a
line is its indentation, plus any YAML `- ` list markers and any of the language's keywords, each
followed by whitespace. The keywords are what make `export NAME=value` in a `.env` file, `ENV
NAME=value` in a Dockerfile and `set NAME=value` in a batch file read their names. A key holds no
whitespace, quote, bracket, brace or comma, and a word the language lists as a keyword, a builtin or a
type keeps its own colour, so `COPY --from=build` colours `--from` as the flag the Dockerfile plugin
names. A `:` ends a key only when a space, a tab, the end of the
line, `=` or another `:` follows it, which is YAML's own rule and keeps `https://` from making `https`
a key while `CC := gcc` in a Makefile still reads `CC`.

`Token::Type` was chosen over a new token. A new token would need a tenth colour in every theme and
every plugin, and the themes plugin refuses a theme that names eight of nine colours for a reason
that would apply to ten as well. In the Dracula scheme every bundled plugin ships, the type colour is
the cyan Dracula uses for keys in its own YAML and TOML schemes.

**`language.sections`**, a flag. A line holding only `[name]` or `[[name]]`, with only whitespace
before it and only whitespace or a comment after it, is one `Token::Keyword`. A row of a TOML array
written across lines ends in a comma, so it is not a section.

**`language.more_line_comments`**, a list of further openers of a comment that runs to the end of the
line. INI comments with `;` and `#`, HCL with `#` and `//`, a batch file with `REM` and `::`. Only the
reading uses them; the comment toggle writes `language.line_comment`. An opener that ends in a word
character opens a comment only when no word character follows it, so `REM` does not swallow
`REMOTE`. Every opener that shipped before this ends in punctuation, so the rule changes nothing for
them.

**The reading stays a function of the text.** Keys and sections look back along their own line, never
further, so `incremental::Tokens`, which starts a reading at the head of a line and stops when the
tokens agree with the previous reading, gives what a whole reading gives. A fuzz of 400 random edits
over a configuration grammar checks it, beside the existing one for a Rust grammar. The look back is
skipped unless the character before the token is whitespace, which is the answer for almost every
token.

### 3.3 The plugins

Fifteen for configuration, build and scripting files, written by hand:

| Plugin | Claims | Uses |
|---|---|---|
| `dockerfile` | `Dockerfile`, `Dockerfile.*`, `Containerfile`, `Containerfile.*`, `.dockerfile` | names, keys (`=`), `-` as a letter |
| `dotenv` | `.env`, `.env.*`, `*.env.example`, `*.env.sample`, `.env` extension | names, keys (`=`) |
| `makefile` | `Makefile`, `makefile`, `GNUmakefile`, `.mk`, `.mak` | names, keys (`=`, `:`) |
| `cmake` | `CMakeLists.txt`, `.cmake` | names |
| `ignore` | `.gitignore`, `.dockerignore`, `.npmignore` and ten more, `.gitattributes` | names only |
| `ini` | `.ini`, `.cfg`, `.inf`, `.desktop`, `.editorconfig`, `.gitconfig`, `.gitmodules`, `.npmrc` | names, keys, sections, `;` and `#` |
| `properties` | `.properties` | keys (`=`, `:`), `#` and `!` |
| `xml` | `.xml`, `.xsd`, `.xslt`, `.xaml`, `.plist`, `.csproj` and the other .NET project files, `web.config` | markup |
| `jsonc` | `.jsonc`, `.json5`, `tsconfig.json`, `tsconfig.*.json`, `.eslintrc`, `.babelrc`, `devcontainer.json`, VS Code's settings files | names, both comment kinds |
| `hcl` | `.tf`, `.tfvars`, `.hcl`, `.nomad` | keys, `#`, `//` and `/* */` |
| `protobuf` | `.proto` | definers |
| `graphql` | `.graphql`, `.graphqls`, `.gql` | definers |
| `powershell` | `.ps1`, `.psm1`, `.psd1` | definers, `<# #>` |
| `batch` | `.bat`, `.cmd` | `REM` and `::`, keys after `set` |
| `groovy` | `.groovy`, `.gradle`, `Jenkinsfile` | definers |

Twelve for the languages the code index already read. Each manifest's language keys are the index's
own manifest with the two keys only the index reads (`definition_tail` and `brace_next_line`) left
out, so the grammar the window builds is equal to the one it built before, which
`the_programming_language_plugins_read_their_files_as_the_code_index_does` checks. What the plugin
adds is the colour scheme and the icon. Ruby also claims `Gemfile`, `Rakefile`, `Podfile`,
`Vagrantfile` and the other Ruby files with no extension.

Four existing plugins changed:

- **YAML** reads keys (`language.keys = :`), which removes the gap its own manifest named, and claims
  `.clang-format`, `.clang-tidy`, `.yamllint` and `.condarc`.
- **TOML** reads keys and sections, which removes its section header gap, and claims `Cargo.lock`,
  `Pipfile`, `poetry.lock` and `uv.lock`.
- **Shell** claims `.ksh` and the start up files (`.bashrc`, `.zshrc`, `.profile`, `.envrc` and the
  rest).
- **JSON** claims `.jsonl`, `.ndjson`, `.geojson`, `.webmanifest`, `.har`, `.avsc` and `composer.lock`.

Every one carries the Dracula scheme the bundled plugins share and an icon made the way the others
were: a Krea 2 render through the AI service, cut out and scaled by `examples/plugin_icon.rs`. The
marks are generic shapes rather than the projects' own logos, so no icon copies a trademark.

### 3.4 The status bar

`file_kind::kind_name` is a table of its own, because it is asked where no plugin list is at hand.
It gained `kind_by_name` for the names the plugins claim and rows for every new extension, and a
`.ps1` file is "PowerShell" rather than "Batch". `every_file_a_new_plugin_claims_is_named_in_the_status_bar`
walks every name and extension the new plugins claim and fails when the table calls one "Plain text",
which is what keeps the table and the manifests in step and what keeps Bold and Italic off them.

### 3.5 What an agent sees

Plugins are data, so nothing new is needed for an agent to reach them: `plugins list` lists them,
`plugins info <id>` describes them, `plugins disable <id>` switches one off, and `editor text` and
`status` report the kind the status bar shows. Switching a plugin off withdraws its colours in the
same frame, exactly as for the plugins that shipped before.

## 4. Left out, with the reason for each

- **Embedded languages inside a Dockerfile RUN line, a Makefile recipe or a GitHub Actions `run:`
  block.** The shell inside is not marked by any element the markup reading could follow, and a
  Dockerfile does not say which shell its image has.
- **Heredocs and multi line strings** in Dockerfile, HCL, GraphQL and the other languages. The string
  rule ends at a line break for every quote but a backtick, which is a tokeniser rule and is named in
  each manifest's limitations.
- **Folder patterns** such as `.github/workflows/*.yml` or `.vscode/settings.json`. See 3.1.
- **Nginx, Apache and systemd `.conf` files.** Each is a different format behind one extension, and
  colouring them as INI would be wrong more often than right. `.conf` is still "Settings" in the status
  bar.
- **Go to Definition in CMake.** CMake writes a function's name inside its brackets, `function(name)`,
  and a definer reads the word directly after the keyword. Reading it would be a rule for one
  language.
- **Running and debugging** any of the new languages. `plugins::DEBUGGERS` drives lldb and node only,
  and a Debug button that did nothing would break the rule that a control is absent when it cannot
  apply.

## 5. Tests

- `unluminous-core`: the key, section and comment rules, the word boundary after an opener that ends
  in a letter, a grammar with none of the keys reading as before, and the incremental fuzz.
- `unluminous-app`, `services::plugins::bundled`: which plugin every one of 69 file names is given to,
  that the grammar list agrees, that names are not mistaken for extensions, that each new plugin
  colours what its description says, that XML reads tags as types, that the definers find what they
  promise, that a pattern is checked, that the twelve languages read as the index does, and that every
  claimed file is named in the status bar.
- The real window: a release build opened on a folder holding one file of each type, each file opened
  through `unluminous-cli`, its plugin and its kind read back, and the window photographed.
