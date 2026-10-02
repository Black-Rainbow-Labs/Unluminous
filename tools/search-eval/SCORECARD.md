# Scorecard: the Unluminous code index against ripgrep

`task-2139` built the code index that `tasks/task-2138-unluminous-code-index-tdd.md` designs and measured
it with this folder's harness. This is the result of the held out gate, the record of how it got there,
and what the result means. Every number here is read out of a run folder under
`D:/unluminous-search-eval/runs`, named beside it.

## The outcome

| Goal | Bar | Result | Verdict |
|---|---|---|---|
| G1 speed | at least 6.0x, lower end of the 95% interval at least 6.0x, on the medium and large corpus | 26.7x to 36.1x on the three local corpora and 128x to 136x on Linux, in three held out runs from cold hosts; lowest lower end 24.82x | **passes** |
| G2 tokens | the index arm's agent session tokens at most 0.50x the rg arm's | 0.95x (0.88 to 1.03) on Sonnet, 0.98x (0.88 to 1.09) on Opus | **not reachable by construction** (below) |
| G3 accuracy | agent success at least min(1.10x rg, rg + half its misses); no family worse; every exact family equal to rg | agent success 0.689 against rg 0.678 on Sonnet (bar 0.745), 0.702 against 0.736 on Opus; exact families equal or better | **not met** |

The goals and the query sets were frozen before the first run and have not been changed. TDD section
8.4 says that when a goal cannot be reached the scorecard says so with the numbers; this is that.

## G1: speed

Each run stopped every index host first, so the index was loaded from disk; embedding was off; both
arms were pinned to the performance cores; and the ripgrep reference queries were checked against the
quiet reference, so a run on a busy machine would not have been graded. The ratio is the geometric
mean of paired per query ratios, `rg` being the ripgrep inside `claude.exe` run with the Grep tool's
own flags, and the index answering over MCP.

| Run | unluminous (small) | ai-service (medium) | inillucent (medium) | linux (large) |
|---|---|---|---|---|
| 1790891400-522094b | 27.76x (25.80 to 29.81) | 36.07x (32.36 to 40.43) | 29.07x (26.76 to 31.55) | 133.64x (96.96 to 181.52) |
| 1790892401-522094b | 26.72x (24.82 to 28.71) | 34.91x (31.29 to 39.09) | 28.62x (26.40 to 31.01) | 135.88x (98.09 to 185.24) |
| 1790893371-522094b | 26.72x (24.85 to 28.64) | 33.18x (29.87 to 36.99) | 29.00x (26.69 to 31.53) | 128.36x (93.18 to 174.37) |

Medians in the last run: rg 39.6, 80.7, 34.6 and 996.3 ms; the index 1.10, 1.21, 1.25 and 7.02 ms.

### The exact families on the held out split (run 1790893371-522094b)

| Corpus | Family | n | rg | index |
|---|---|---:|---:|---:|
| ai-service | F1 definition at rank 1 | 48 | 0.208 | 0.500 |
| ai-service | F2 exact set of lines | 79 | 1.000 | 1.000 |
| ai-service | F3 recall of uses | 36 | 0.715 | 0.715 |
| ai-service | F4 file in the top 3 | 4 | 0.000 | 0.500 |
| inillucent | F1 | 50 | 0.360 | 0.780 |
| inillucent | F2 | 100 | 1.000 | 1.000 |
| inillucent | F3 | 44 | 1.000 | 1.000 |
| inillucent | F4 | 24 | 0.583 | 0.583 |
| linux | F2 | 75 | 1.000 | 1.000 |
| unluminous | F1 | 52 | 0.346 | 0.827 |
| unluminous | F2 | 94 | 1.000 | 1.000 |
| unluminous | F3 | 40 | 0.997 | 0.997 |
| unluminous | F4 | 2 | 0.500 | 0.500 |

F2 is grep patterns that agents really ran, mined from 1,076 transcripts, and the index returned exactly
ripgrep's lines for every one of them on all four corpora.

### F7: freshness

A scratch clone of each corpus was changed and searched at once through both arms: a new file, an edited
line, a rename, a delete, a burst of 5,000 files, a branch switch, and five rounds of writing and
searching with no pause. Stale or missing results: **0** on unluminous, inillucent and ai-service
(`freshness-*-17908944*.json`, `freshness-ai-service-1790894611813.json`).

## G2: tokens, and why the bar cannot be reached

| Run | Model | Tasks x draws | rg median tokens | index median tokens | Paired ratio |
|---|---|---|---:|---:|---|
| agents-1790894470-522094b | Sonnet | 121 x 3 | 44,024 | 47,065 | 0.95 (0.88 to 1.03) |
| agents-1790901934-6a86c10 | Opus | 121 x 1 | 66,556 | 58,501 | 0.98 (0.88 to 1.09) |
| agents-1790885731-522094b (dev) | Haiku | 40 x 2 | 577,684 | 529,496 | 0.77 (0.58 to 1.02) |

G2 measures the whole agent session. Sonnet and Opus finish these tasks in five to seven turns in
either arm, and each turn sends the same instructions and tool definitions again, about 5,200 tokens at
the first turn in both arms. Their Grep calls are already narrow: a Grep that lists files returns 60 to
850 characters. The search answers are a small share of a session, so no change to them can halve its
total. Haiku makes three times as many turns and its sessions reach hundreds of thousands of tokens,
which is where the index saves most; three two draw runs on Haiku gave 0.80, 0.75 and 0.77, and
the question family alone never went below 0.67. The goal was not redefined.

## G3: accuracy

### Agent level, held out

| Model | Arm | Success | F5 tickets | F6 questions |
|---|---|---:|---:|---:|
| Sonnet, 3 draws | rg | 0.678 | 41/138 | 205/225 |
| Sonnet, 3 draws | index | 0.689 | 42/138 | 208/225 |
| Opus, 1 draw | rg | 0.736 | 17/46 | 72/75 |
| Opus, 1 draw | index | 0.702 | 16/46 | 69/75 |

The bar on Sonnet is min(1.10 x 0.678, 0.678 + half of the misses) = 0.745. The index arm is within one
or two sessions of rg in every family and corpus, so no family is worse; it does not reach the bar.
Two things put the bar out of reach on this set:

- F6 is near the ceiling in both arms: Sonnet with ripgrep answers 91% of the questions.
- F5 counts a success when half of the files a ticket changed are among the first five the agent names.
  15 of the 46 held out tickets changed more than ten files, so 45 of the 138 F5 sessions cannot
  succeed in either arm. On the tickets that can, rg solves about 44%, and the bar would need about 70%.

### Tool level, held out: F5, F6 and F8 (meaning-1790909171-6a86c10)

The index arm is one plain English `find`. The rg arm is every Grep call the Sonnet agent made for the
same task in the gate run, replayed against the same folder, so it is several searches written by a
model that had read the code against one search written from the ticket or the question alone.

| Corpus | F5 index | F5 rg replay | F6 index | F6 rg replay | F8 index |
|---|---:|---:|---:|---:|---:|
| unluminous | 0.020 | 0.310 | 0.014 | 0.109 | 0.857 |
| ai-service | 0.101 | 0.029 | 0.084 | 0.000 | 0.500 |
| inillucent | 0.165 | 0.217 | 0.046 | 0.166 | 0.636 |

One plain English search is worse than the agent's own Grep searches on two corpora of three, in both
families, so the tool level half of G3 is not met either. F8 has no rg arm: ripgrep has no way to say
that the code holds no answer, and on the held out questions the index said so for 50% to 86% of them.
In the agent runs this did not show as a loss, because an agent with the index also ran `find`, `def`
and `refs` with words it had read in the code; but a single search from the words of a ticket is
weak, and it is the part of this design with the most room left.

## What changed on the way, in order

The full record, with every run, is `D:/unluminous-search-eval/progression.md`. In short:

1. **Parity first.** The file set is the Grep tool's; UTF-16 files are decoded; a binary file named
   outright keeps its real line numbers; the regex planner stopped joining text across a wildcard.
   Parity with rg on the dev F2 set went to 899 of 900, and the last pattern is one ripgrep refuses too.
2. **Speed.** Posting lists of 64 KB blocks and in place verification: 3.5 ms to 1.3 ms on a query
   with no hits on ai-service. Compaction moved off the index lock after F7 found a 19 second stall.
3. **Inillucent 2.0.2.** Vectors written a batch per transaction: from 4 to 22 to 37 chunks a second
   per host. A passage rebuild stopped holding the index lock.
4. **Questions in plain English.** Code ranked before documents (F6 0.15 to 0.21 with no cutoff); the
   answer turned away by word coverage at 0.31, because `confidence()` does not separate the cases on
   code; five fusion settings compared and found equal.
5. **Fewer reads and fewer wasted turns.** Small answers carry the code around their hit; a short
   `files` answer carries an outline; `def` takes `Type::name`; a folder named by its last part is
   found; a refusal names the keys it ignored. Each came from reading an agent transcript.
6. **Large repositories.** The processor embeds at most 100,000 chunks, and `UNLUMINOUS_EMBED=off`
   stops embedding during a timed run.

## What this means

As a search engine the index does what it was built to do: exactly ripgrep's lines, 27 to 136 times
faster, fresh after every kind of change, with a definition found at the first place a little more than
twice as often as a whole word ripgrep search finds it there. As a tool for an agent it is as accurate as ripgrep and costs
about the same: a capable model already uses ripgrep well enough that the cost of a session is its
instructions rather than its searches.
