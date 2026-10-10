# task-2237: a learned completion ranking that beats IntelliJ

## 1. Introduction

`task-2232` built the three tiers `tasks/task-2231-autocomplete-intellisense-tdd.md` designs: the words
of the file and the project, the structure Atrius reads, and rust-analyzer and tsserver. It ranks what
they offer with a fixed chain of weighers. On the held out positions of `tools/completion-eval/` it
scored, for the expected name first in the list (R@1):

| Language | Unluminous 0.71.0 | IntelliJ, ML ranking on | IntelliJ, ML ranking off |
|---|---|---|---|
| Rust | 56.9 | 66.6 | 47.9 |
| TypeScript | 40.1 | 58.1 | 42.1 |

Unluminous is ahead of IntelliJ's own heuristics and behind its trained model. This ticket asks for a
score better than IntelliJ's default. This document designs how: the ranking is learned from the
evaluation's tune positions with features IntelliJ's model is known to read and the chain does not,
and the pool is widened where the right name is missing from it entirely.

## 2. Goals and non goals

### Goals

| # | Goal | Bar |
|---|---|---|
| G1 | **Better than IntelliJ ML on.** | On the held out positions, run once at the end: R@1 and MRR above `baseline-intellij-ml-on` for Rust and for TypeScript. |
| G2 | **No class falls behind where it was.** | No class of either language loses more than 3 R@1 points against `gate-unluminous-on`. |
| G3 | **The keystroke stays under budget.** | `examples/completion_cost.rs` on `app/realm.rs`: the worst stem stays under 5 ms. |
| G4 | **One ranking for every reader.** | The popup, `editor complete` and the harness read the same `completion::order`. `editor complete --explain` shows each row's features and score. |

### Non goals

- A neural model or a language model over the file. TreeRanker (section 9) shows one can beat
  IntelliJ's ranker, at 66 ms a ranking on a graphics card. This design stays on the processor and
  inside the keystroke budget.
- Collecting people's selections to train on. The statistics `task-2232` keeps are a feature; the
  training data is the evaluation's tune split.
- Changing the positions, the classes or the scorer of the harness. The positions are frozen.

## 3. What the numbers say

Read from the held results of `task-2232` (aggregates only; no held transcript was read):

| Language | Prefix | Unluminous R@1 | IntelliJ R@1 | Unluminous miss | IntelliJ miss |
|---|---|---|---|---|---|
| Rust | 0 | 21.4 | 27.8 | 29.4 | 22.2 |
| Rust | 1 | 53.1 | 70.5 | 6.0 | 4.4 |
| Rust | 2 | 72.6 | 82.9 | 2.9 | 3.8 |
| Rust | 3 | 81.7 | 86.2 | 2.5 | 3.4 |
| TypeScript | 0 | 12.8 | 34.9 | 48.9 | 25.7 |
| TypeScript | 1 | 34.3 | 56.6 | 15.2 | 16.5 |
| TypeScript | 2 | 53.1 | 67.7 | 6.5 | 13.8 |
| TypeScript | 3 | 60.7 | 73.8 | 5.4 | 11.0 |

"Miss" is the share of queries whose expected name is not in the first 50 rows. Two kinds of loss
follow:

1. **Order.** At one to three letters the right name is in the list as often as IntelliJ's, often
   more often, and lower in it. This is the ranking.
2. **Recall with nothing typed.** At prefix 0 a quarter of Rust queries and half of TypeScript queries
   have no right answer in the list at all. A list asked for with nothing typed holds the locals,
   this file's definitions, the server's rows and the keywords, and the server's rows for TypeScript
   are sorted into groups that put hundreds of global types before the file's own names.

## 4. The design

### 4.1 Features the chain does not read

IntelliJ's ranking model (`plugins/completion-ml-ranking`, and Bibaev et al. 2022) reads, beside the
heuristic weighers' own outputs, how far the caret is from the element (`lines_diff`), how similar
the candidate is to the words on the caret line and in the enclosing block (`line_max`,
`parent_max`), the project's n-gram model, recent places, and session history. The cache language
model literature (Tu, Su and Devanbu 2014; Hellendoorn and Devanbu 2017) says the same thing another
way: the identifiers written near the caret are the likeliest to be written again.

Each row gains these facts, all read from the file that is showing, in one pass over the places the
name is written (`app::completion::mark_the_names_written_here`):

| Field | Meaning |
|---|---|
| `uses_here` | how many times the name is written in the file, the word being typed left out |
| `lines_above`, `lines_below` | lines from the caret to the nearest place above and below |
| `uses_near` | places within 30 lines of the caret |
| `same_before` | places that follow the token that precedes the word here (`let` `mut`, `.` `push`, `->` `Self`) |
| `same_after` | places followed by the token that follows the word here (`(`, `::`, `:`) |
| `words_nearby` | the share of the name's words (`draw_frame` is `draw` and `frame`) written on the three lines above the caret and the one below |
| `offered_by` | every source that offered the spelling, as bits; a name the server, this file and the project all offer is a surer answer |

The tab already reads where every word is written once a text revision; it now keeps the offsets
(`TabSymbols::places`) and the line starts rather than only a count.

### 4.2 A learned score in place of the chain

`completion::order` sorts by a key. Today the key is a tuple of the chain's weighers. With this
change the key is:

1. a server's preselected row, when the stem is its prefix (unchanged);
2. the match class: exact, prefix, humps, word start, subsequence (unchanged, because a person who has
   typed `lay` and sees `Layout` below `replay` reads the list as broken, whatever a model says);
3. **a learned score**, higher first;
4. the name's bytes, so the order is total.

The score is a gradient boosted ensemble of shallow regression trees over the features in 4.1 and the
facts the chain already reads (case of the first letter, expected type, locality, source, kind
against the place, the server's own order turned into a rank within the query and, for
rust-analyzer, its relevance score, how often the row was chosen before, deprecation, the alignment
score and the length). It is trained listwise: for each query the target is the expected name, and
the loss is the softmax cross entropy over that query's rows, which is CatBoost's `QuerySoftMax` that
IntelliJ's own models use.

The trees are written into `crates/unluminous-core/src/completion/model.rs` as constant arrays by the
trainer, so the model is code: deterministic, the same on every machine, and with no file to load.
Evaluating 100 trees of depth 5 is about 500 comparisons a row; at the 1,125 rows of the worst stem on
`app/realm.rs` that is well under a millisecond.

The chain is kept as the fallback for rows the model was not trained for (an import list, which
`rank_all` orders, and a notebook kernel's rows), and every test that names a weigher still reads
what the chain decides there.

### 4.3 The data

- **Training data is the tune half of the frozen positions**, gathered by running the window over
  it with `editor complete --explain` (`engine-unluminous.mjs --explain 100`). Each query writes its
  first 100 rows and every feature into `pools.jsonl`.
- The tune half is split again, by a hash of the position id, into 80% to fit and 20% to choose the
  model's size and stop its training. The held half is read once, at the end.
- The trainer is `tools/completion-eval/rank-model/` (Node, no dependencies). The same code scores a
  pool file offline, so a ranking change is measured in seconds against the pools a window gathered,
  and a window run confirms it.

### 4.4 Recall with nothing typed

The trainer can only reorder what the pool holds. Two changes to the pool, each measured on the tune
pools before it is kept:

- With nothing typed outside a member access, the file's own words are offered as well, ranked by the
  model. They are the cache language model's candidates and the largest single source of the
  expected names that are missing.
- A server row is given the facts in 4.1 like any other, so a global DOM type that is written nowhere
  in the file ranks below the file's own interface.

## 5. Testing

- `editor complete --explain` has a command line test, a catalogue row and its section in
  `unluminous-cli/docs/commands.md`.
- A core test checks that the model's score in Rust equals the trainer's score for a fixed set of
  feature rows exported with the model, so the code and the trainer cannot drift.
- The existing weigher tests are kept where they still describe the order, and rewritten where the
  model now decides; each rewritten test asserts an order a person would expect.
- `examples/completion_cost.rs` is run before and after.
- The hill climb record (baseline, each round's change, the tune and validation numbers) is in
  `_agent_output/task-2237/hillclimb/`, and the held result goes into
  `tools/completion-eval/SCORECARD.md`.

## 6. Risks

| Risk | Answer |
|---|---|
| The model learns the corpora's names rather than how completion works. | No feature is a name. Every feature is a count, a distance, a class or a flag, so nothing about a particular corpus can be memorised beyond its statistics. |
| Overfitting to the tune positions. | The validation fifth of the tune half picks the model, and the held half is read once. |
| A server's answer differs between runs, so a pool is not exactly reproducible. | The confirming runs are real window runs, and the final number is a real window run. |
| The order surprises a person: a subsequence match above a prefix match. | The match class stays ahead of the score. |

## 7. What was built, and where it differs from this design

Written after the work, from the hill climb record in `_agent_output/task-2237/hillclimb/`.

- **The empty stem was a fault, not a pool choice.** `completion::could_match` answers false for an
  empty stem, and `completion_candidates` used it for this file's definitions, its words and the
  keywords. So a list asked for with nothing typed held the enclosing function's locals and the
  server's rows and nothing else. It now holds this file's definitions, its words and the keywords;
  another tab's definitions and the project's names still need a letter.
- **The order.** A server's preselected row, then the match class, then the model's score, then the
  chain's rank. While fewer than four letters are typed (`EXACT_FIRST_FROM`), a name exactly equal to
  the stem is in the prefix group rather than ahead of it: with `fi` typed, a project function called
  `fi` was first whatever the model said, and taking that rule out was the largest single gain of the
  climb (Rust 73.6 to 77.3, TypeScript 71.3 to 81.6, offline). From four letters the exact name comes
  first, so `Enter` on a word typed in full is still a new line.
- **What the model scores.** The chain's first 100 rows and the 100 written nearest the caret
  (`SCORED_BY_THE_CHAIN`, `SCORED_BY_NEARNESS`). A 450 tree model scoring every row cost 47 ms a
  keystroke; 133 trees on at most 200 rows cost 3.9 ms on `app/realm.rs`'s worst stem.
- **Features beyond 4.1.** The kind of token before the word and after it on its line
  (`completion::token_class`). The token after the caret is read on its own line only, because a person
  typing new code is at the end of a line, and the model is trained on every query twice, once with
  nothing after the caret, so it ranks well there too.
- **Members.** After `.` or `::` the list also offers the words this file writes straight after that
  separator. A library member such as `map` or `max` is not in the structure, and with tsserver slow
  it was not in the list at all.
- **Imports.** Import lists are ordered by the model too.
- **The data.** `editor complete --explain` reports each row's `chainRank`, so pools can be gathered
  from a build that already has a model. The engine runs one window per corpus, three at a time,
  confined to 12 of the 24 processors at below normal priority (`D:/unluminous-completion-eval/capped.ps1`).

## 8. Sources

- Bibaev et al., "All You Need Is Logs: Improving Code Completion by Learning from Anonymous IDE Usage
  Logs", ESEC/FSE 2022. https://arxiv.org/abs/2205.10692
- `intellij-community/plugins/completion-ml-ranking`: `CommonElementLocationFeatures.kt`
  (`lines_diff`), `ContextSimilarityFeatures.kt`, `RecentPlacesFeatures.kt`, `ngram/`, `MLSorter.kt`.
- rust-analyzer `ide-completion/src/item.rs` (`CompletionRelevance`) and `lsp/to_proto.rs`
  (`sortText` is the score XOR `0xFFFFFFFF`).
- TypeScript 5.6 `SortText` (`"10"` locals to `"18"` JavaScript identifiers).
- Cipollone et al., TreeRanker, 2025. https://arxiv.org/html/2508.02455v1
- Tu, Su and Devanbu, "On the localness of software", FSE 2014.
- The research notes for this ticket: `_agent_output/task-2237/research.md`.
