# Agent accuracy

How often an AI agent driving Unluminous through its MCP tools does the job it was asked, with the
right command the first time and without falling back to its own shell. Measured on a local model,
so it can be run on every change without spending a hosted model's usage, and recorded here so the
numbers can be watched over time.

## The numbers

Each row is 52 cases run twice (104 conversations) by opencode on the local Qwen 3.8 27B, on
2026-10-20. Each row is the change on top of the rows above it that were kept.

| Build | Change | Success, test | Success, training | Clean | First call right | Refused a case | Own reads a case | Own writes a case | Calls a case | Seconds a case | Kept |
|---|---|---|---|---|---|---|---|---|---|---|---|
| 1.3.2 | as shipped | 87% ±12 | 82% ±9 | 48% ±10 | 91% | 0.72 | 0.71 | 0.27 | 5.8 | 43 | |
| v1 | the server reads a call the way it was meant where there is one reading | 80% ±14 | 86% ±8 | 54% ±10 | 96% | 0.19 | 0.65 | 0.20 | 5.0 | 38 | yes |
| v2 | an example call under every command in the descriptions | 77% ±15 | 86% ±8 | 57% ±10 | 96% | 0.29 | 0.63 | 0.15 | 4.8 | 45 | no |
| v3 | a tool for each plugin that draws, `--path` on editor and fold commands, `git diff` | 90% ±11 | 92% ±6 | 55% ±10 | 89% | 0.30 | 0.66 | 0.05 | 4.7 | 66 | yes |
| v4 | the instructions name the tool that does what `read`, `grep` and `edit` would | 77% ±15 | 85% ±8 | 67% ±9 | 98% | 0.13 | 0.50 | 0.14 | 4.1 | 58 | changed in v5 |
| v5 | v4 with each tool named by its area | 87% ±12 | 89% ±7 | 66% ±9 | 95% | 0.23 | 0.46 | 0.22 | 4.4 | 72 | yes |
| v6 | `unluminous_batch`, several command lines in one call | 83% ±13 | 92% ±6 | 64% ±9 | 95% | 0.36 | 0.37 | 0.05 | 4.5 | 40 | yes |
| v7 | the Unluminous skill, which `mcp install` and `setup` write | 93% ±9 | 93% ±6 | 66% ±9 | 92% | 0.36 | 0.26 | 0.11 | 4.8 | 43 | yes |
| v8 | three server faults found in the v6 refusals: only an object is sent as `structuredContent`, a narrow tool reads keys wrapped in `arguments`, and the tool with no area sends an area's name on to that area's tool | 83% ±13 | 92% ±6 | 64% ±9 | 96% | 0.26 | 0.25 | 0.06 | 4.7 | 42 | yes |
<!-- next row -->

What moved, from 1.3.2 to v8: success on the held out cases went from 87% to between 83% and 93% (v7
and v8 differ by less than the interval) and on the others from 82% to 92%; clean went from 48% to 64%;
refused calls fell from 0.72 a case to 0.26; the agent read the project with its own tools 0.25 times a
case where it had read it 0.71 times; and it wrote files behind the window 0.06 times a case where it
had written them 0.27 times.

v8 is kept on the refusals it removed. The four held out cases that lost a rep against v7 were read in
full, and none of them reached the three fixes: the agent wrote a file with its own tool, chose the side
by side view instead of the preview, put a breakpoint on the line above the one asked for, and did not
use `editor comment`.

Two rows taught something. v2 put an example call under every command and cost 3,900
tokens a request for no gain, so examples are given only in a refusal. v4 named tools by their bare
names in the instructions, and the model copied those names into calls it wrote as text, which
opencode never runs because its names carry a prefix: test success fell to 77% until v5 named each
tool by its area. The seconds column moved with how busy the machine was (v5 72 s, v6 40 s on the same
tools), so read it only against a row run at a quiet time.


## What each number means

| Number | What it counts |
|---|---|
| Success | The job was done, read back from the window's state and the project's files. A question was answered with the facts only the project gives. |
| Clean | Success, with no refused call and no call to the agent's own tools (`bash`, `read`, `edit`, `glob`, `grep` and the rest). This is the number the work is aimed at. |
| First call right | Cases whose first Unluminous call was accepted. |
| Refused a case | Calls the server refused or answered as an error, on average. |
| Discovery a case | Calls that only asked what commands there are (`commands`, `plugins show`, `action list`). |
| Own reads a case | The agent's own `read`, `glob`, `grep` and `ls`: reading the project behind the window. |
| Own writes a case | The agent's own `edit`, `write` and shell redirects: changing a file behind a window that may have it open. |
| Seconds a case | Wall clock for the whole conversation. |

`±` is the 95% interval of a rate. A change smaller than that is not a change.

## How it is measured

The cases are in `tools/agent-study/cases.mjs`: the 37 scenarios of the agent study
(`tools/agent-study/scenarios.json`) and 17 more aimed at commands an agent has to find rather than
guess. Each is a request phrased the way a person says it, such as "I want src/main.rs on the left and
src/shapes.rs on the right", and a check that reads the answer back through `unluminous-cli` and the
disk. Two scenarios are not graded, because a fresh settings folder cannot do them whatever an agent
does: `s34-members-and-signature` needs rust-analyzer, and `s36-dictation` needs the speech model.

The agent is opencode with the local Qwen 3.8 27B, and the only MCP server it has is the Unluminous
one. Each case runs in a fresh window: a new copy of the sample project, a settings folder copied from
a template, and its own `APPDATA`, so a theme, a ticket or a font size cannot carry from one case to
the next and the person's own windows are never touched (`tools/agent-study/window.mjs`).

A third of the cases are held out as the test split, chosen by the name of the case (the FNV-1a hash of
its id is 0 mod 3), so a change made by reading the other two thirds is judged on cases nobody read.

## Running it

Build the build to measure, then run every case twice and print the table:

```sh
cargo build --release -p unluminous-app -p unluminous-cli
node tools/agent-study/eval.mjs --variant v1 --flow _agent_output/accuracy \
  --window-bin target/release --cli target/release/unluminous-cli.exe --reps 2
node tools/agent-study/score.mjs _agent_output/accuracy
```

It needs the local model on `http://localhost:8080/v1` (`--base-url` names another) and `opencode` on
the PATH. A run of 52 cases takes about 40 minutes a rep. A case that could not be run at all goes to
`errors.jsonl` rather than being scored, and running the same command again picks up where it stopped.
`--cases a,b` runs some of them, and `--redo a,b` runs them again after a check is fixed.

Add a case when you add a feature: an entry in `EXTRA` in `cases.mjs` with a check that reads the
window's state back.
