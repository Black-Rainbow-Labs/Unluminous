`crates/unluminous-chat/src/rich` is 2,889 lines in six files. Every file is new and untracked in this worktree.

```ui
{"type": "chart", "kind": "bar", "title": "Lines per file in rich/", "unit": "lines", "labels": ["component.rs", "catalogue.rs", "expr.rs", "repair.rs", "mod.rs", "format.rs"], "series": [{"name": "lines", "values": [1179, 484, 484, 338, 252, 152]}]}
```

```ui
{"type": "table", "title": "What each file does", "columns": ["File", {"label": "Lines", "align": "right"}, {"label": "#[test]", "align": "right"}, "Purpose"], "rows": [["component.rs", 1179, 7, "The 21 component types, read from JSON in two modes: lenient while the block arrives, strict once it is finished"], ["catalogue.rs", 484, 6, "The one list of components. The guide, the reference, the validate field names and the gallery are all built from it, and every example is a test"], ["expr.rs", 484, 5, "The calculator's expression language: parser and evaluator, with limits on steps (STEPS) and nesting depth (DEEPEST)"], ["repair.rs", 338, 7, "Closes JSON that was cut off so a block can be drawn while it streams"], ["mod.rs", 252, 7, "The module's entry point: finds the ui fences and ties the other files together"], ["format.rs", 152, 3, "How numbers are written: number, integer, money, percent, compact"]]}
```

The #[test] column is a count of `#[test]` lines in each file.

**Review in this order:**

1. **`expr.rs`**: it evaluates expressions the model writes, so it decides what model text can do. Check that the `STEPS` and `DEEPEST` limits hold on every recursive path, and check division by zero, NaN and infinity. It has the fewest tests for its size.
2. **`repair.rs`**: it runs on every partial block while an answer streams. A panic here would come from text that is still being cut short. Check escape sequences cut in the middle (`\u12`), multibyte UTF-8 cut in the middle, and numbers like `-` or `1e`.
3. **`component.rs`**: the largest file. Check that the lenient and strict readings agree once a block is complete, and that a required field that is missing produces a notice and never a panic.
4. **`catalogue.rs`**, **`mod.rs`** and **`format.rs`**: mostly data and glue, and the catalogue's examples are already tested.

```ui
{"type": "files", "items": [{"path": "crates/unluminous-chat/src/rich/expr.rs", "note": "Review first: evaluator limits"}, {"path": "crates/unluminous-chat/src/rich/repair.rs", "note": "Partial JSON edge cases"}, {"path": "crates/unluminous-chat/src/rich/component.rs", "note": "Lenient vs strict reading"}]}
```

```ui
{"type": "actions", "items": [{"label": "Review expr.rs for bounds and panics", "send": "Review crates/unluminous-chat/src/rich/expr.rs for any path that escapes the STEPS or DEEPEST limits, or that panics", "primary": true}, {"label": "Fuzz repair.rs", "send": "Write a test that feeds repair.rs every prefix of each catalogue example with multibyte and escaped strings, and report any panic"}, {"label": "Open expr.rs", "open": "crates/unluminous-chat/src/rich/expr.rs", "line": 1}]}
```
