---
name: unluminous
description: Drive the Unluminous editor window open on this machine through its MCP tools, one tool for each area (tab, editor, terminal, git, search and the rest). Use whenever the person asks to open, edit, save, search, fold, split, highlight, run, debug, or change a setting in Unluminous, or to do something "in the editor", so the work happens in their window instead of behind it with your own file tools.
---

# Driving Unluminous

Unluminous's MCP tools drive the Unluminous window the person is looking at. Everything a person can do there has a command. Use the commands rather than your own `read`, `edit`, `write`, `grep` or `bash`: a file you write yourself is written behind a window that may have it open, and the person does not see it happen.

**Each tool is named after its area.** The tab tool's name ends in `unluminous_tab`, the database tool's in `unluminous_database`, the batch tool's in `unluminous_batch`. Your client puts its own prefix in front of those names, so call each tool by the name your tool list shows.

## How a call is written

An area tool takes the command in `command` and everything else in `arguments`. The usage line `tab open <path> [--transient]` is this call to the tab tool:

```json
{"command": "open", "arguments": {"path": "src/main.rs"}}
```

- A word in `<angle brackets>` or `[square brackets]` is a key of `arguments`, named as written: `<path>` is `"path"`.
- A `--flag` is a key without the dashes. A flag with no value is `true`: `--all` is `"all": true`.
- A relative path is resolved against the project folder, not against your working directory.
- Numbers are numbers: `"line": 30`, not `"30"`.

The editor and fold commands act on the file that is showing, and each takes `path` to name the file instead: `{"command": "comment", "arguments": {"path": "src/shapes.rs"}}` to the editor tool shows that file first, opening it when it is not open. Read a file with the editor tool's `text` and `path`, change it with `insert`, `replace` or `set-text`, then save it with the tab tool's `save`.

## Which tool

| The person says | Tool, and its commands |
|---|---|
| open, close, save, show a file | **tab**: `open`, `close`, `save`, `show`, `list`, `reopen` |
| type, insert, select, move the caret, go to a line, comment, duplicate a line | **editor**: `insert`, `select`, `caret`, `comment`, `lines`, `text` |
| change every X to Y in a file | **editor_replace** |
| rename a function or a type everywhere | **editor_rename** |
| where is X defined, where is it used | **editor_definition**, **editor_references**, or **search** with `def` and `refs` |
| find text or a file in the project | **search**: `find`, `files` |
| fold or unfold code | **fold** |
| side by side, split view | **pane**: `split` |
| make a file or folder, move or rename a file | **explorer**: `new-file`, `new-folder`, `move` |
| run a shell command | **terminal**: `new`, `send`, `read` |
| run or debug the project | **run**, **debug** |
| git status, what changed, commit | **git**: `status`, `diff`, `action` |
| a setting (font size, indent, line numbers, keymap) | **settings**: `get`, `set`, `list` |
| a theme | **theme**: `list`, `set` |
| a menu entry by its name | **action**: `find`, then `run` |
| a dialog | **modal** |
| a panel's place or size | **panel** |
| what the window looks like | **window**: `screenshot` |
| the Realm canvas | **realm** |
| the Agent-Tasks board | **agent_tasks** |
| the Agent-Chat pane, or an Agent Chat node (give `node`) | **agent_chat** |
| a database: data sources, tables, SQL | **database** |
| the Designer | **designer** |
| install, remove, make or edit an agent skill (SKILL.md) | **skills**: `list`, `install`, `remove`, `new`, `edit` |
| an Excel or CSV workbook: cells, formulas, formats, sheets, charts | **spreadsheet**: `open`, `read`, `write`, `format`, `save` |

## When a call is refused

The refusal says what was wrong, names what the command takes, and ends with a call to that command that works. Read it and fix the call; do not send the same call again. When you do not know a command's name, the action tool's `find` looks up a menu entry by its words.

## Several steps at once

The batch tool runs several command lines in order and answers with each one's result, stopping at the first that fails:

```json
{"steps": ["tab open src/main.rs", "tab open src/shapes.rs", "pane split"]}
```

Each step is written exactly as its usage line shows it, without `unluminous-cli` in front. It is also the way to send one command whose nesting is hard to get right: `{"steps": ["plugins run database add-source sample data/sample.db"]}`.
