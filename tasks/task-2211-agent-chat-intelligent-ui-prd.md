# task-2211: Agent-Chat answers that are interfaces (PRD)

## What was asked

> Deep research https://openai.com/index/gpt-6-for-everyone/. We want our agent chat to have the same
> features, and black rainbow labs rux to facilitate the base components. Agent chat should have
> messages with a rich component set that the agent is easily familiar with and has knowledge of, with
> a good example set and tool set. [...] Agent chat should be made world class design. Get rid of the
> what can I help you with opening prompt suggestions. Instead provide a stellar world class
> experience. Deeply analyze screenshots in various states, zoom levels, panel arrangements.

## What OpenAI shipped, read from the page

The page is "GPT-6 and Intelligent UI for everyone" (October 2026). A copy of its text is in
`_agent_output/task-2211-agent-chat/openai-gpt6-intelligent-ui.md`. The parts that matter here:

1. **An answer is composed of text, visuals and interactive elements**, and the model chooses how they
   fit together for each question. The page names graphics, tappable buttons, forms, charts, maps,
   side by side comparisons, interactive diagrams, timelines beside a plan, and small tools the model
   builds for the question: a savings calculator, a bill splitter, a game.
2. **A plain text answer is still an answer.** The model is trained to know when text is enough.
3. **A library of native, streamable components plus a compiler.** The library gives every answer the
   same design foundation; the compiler builds the interface progressively while the model is still
   writing, so nothing waits for the end of the answer.
4. **The model was trained on the library**, and its interfaces were graded for clarity, usefulness and
   completeness.
5. **Next questions are offered as buttons at the end of an answer** ("Help me adjust the lamb roast
   plan for 5 guests"). Pressing one sends it.
6. **Thinking and answering are interleaved**: the answer starts while the model is still working.

What does not carry over to Unluminous: a map (Unluminous fetches nothing from the network, and a map
is tiles from a server), and pictures fetched from the web for the same reason.

## Who this is for

A person working in Unluminous with an agent beside their code (Claude Code, Codex, or a model at an
address), and the agent itself. The agent is the author of every component, so the second reader of
this document is the agent: it has to know the components exist, what each one is for and exactly how
to write one.

## Requirements

### R1. A component library the agent writes in its answer

- The agent writes a component as a fenced block whose language is `ui`, holding one JSON object.
  Markdown around it is drawn as it is today. JSON because every model writes it correctly, it is
  what tool arguments are already written in, and a fence keeps the answer readable as plain text in
  a terminal, a transcript or a copy.
- The library has these components, grouped by what they are for:

  | Group | Components |
  |---|---|
  | Layout | `card`, `columns`, `tabs`, `stack` |
  | Explaining | `callout`, `steps`, `timeline`, `keyvalue`, `badges`, `diagram` |
  | Numbers | `stats`, `progress`, `chart` (bar, line, area, donut), `table` |
  | Code | `files`, `diff` |
  | Acting | `actions` (buttons), `choices`, `checklist`, `form`, `calculator` |

- Every component draws with the rux library's base controls and the pane's dark neumorphic palette,
  so a component looks like it belongs to Unluminous and every answer shares one design.
- An action never runs anything without a press. The actions a component can take are: send a
  message, open a file at a line, copy text, and fill the composer. Nothing else. A component cannot
  run a command, cannot fetch anything and cannot change a file.
- A `calculator` is how the agent builds a small tool for the question: number inputs and sliders,
  outputs computed from them with arithmetic expressions, and optionally a chart computed over a
  range. The expressions are evaluated by Unluminous with no access to anything but the inputs.

### R2. Components appear while the answer is still arriving

- A component is drawn from the moment its fence opens, and it fills in as the JSON arrives: a table
  gains rows, a chart gains bars, a list gains items. Unfinished JSON is closed by Unluminous before
  it is read, so a half written answer draws everything complete so far.
- A component that cannot be read once the answer has finished is drawn as a quiet notice saying
  why, with the source behind a disclosure. It never makes the rest of the answer disappear.

### R3. The agent knows the library and can check its own work

- The first question of every conversation tells the agent that the library exists, with a compact
  reference and one example. A model at an address gets the same text in its system prompt.
- `unluminous-cli plugins run agent-chat components` lists every component with its fields and a
  complete example; `components <name>` gives one. An agent can read the full reference without the
  person seeing it.
- `unluminous-cli plugins run agent-chat validate <json>` says whether a block is valid and, if it is
  not, exactly which field is wrong. This is the agent's way to check a component before it answers.
- `unluminous-cli plugins run agent-chat gallery` puts a conversation into the pane showing every
  component, which is the example set for people and for agents, and what the screenshot tests use.

### R4. The empty pane is a welcome, not a list of prompts

- The four starter chips ("Explain this file", "Find the bug", ...) are removed.
- An empty pane shows: which agent is answering and at what permission, what the agent can see (the
  project and the file showing), the conversations to pick up again, and the keys that matter. It is
  laid out as a composed page with the same depth language as the rest of the pane, and it works at
  every pane width from the narrowest dock column to a maximised pane.

### R5. World class finish

- Every component is checked by looking at it: at the default size, zoomed in and out, in a narrow
  column, a wide pane and a bottom strip, while streaming and finished, and on a canvas node.
- Spacing, type sizes and corner radii come from one set of numbers, so components line up with each
  other and with the bubbles around them.
- A component never draws outside its message, never overlaps the next row, and scrolls with the
  conversation without its shadow leaking over the header or the composer.

### R6. The AI first rule

Everything a person can do with a component, an agent can do through `unluminous-cli`: press an
action, tick a checklist item, set a form field or calculator input, submit a form, and read back what
a component shows (`plugins view agent-chat` reports each component's kind and state).

## Out of scope, with the reason

- Maps and web pictures: Unluminous fetches nothing.
- Components that run commands: a chat answer is not allowed to act on the machine without the person
  pressing something, and the actions the library offers are the ones whose worst case is a message.
- Training a model on the library: Unluminous does not train models. The reference, the examples and
  the validator are what stand in for it.
- Persisting a checklist tick or a form value across a restart: they are kept for the life of the
  window. Writing them into the transcript would change what goes back up the wire.

## How it is verified

- Unit tests over the parser (complete, partial at every byte, invalid), the expression evaluator, and
  each component's measured height.
- Screenshot tests of the gallery at three widths and three zooms, a streaming answer cut at several
  points, the welcome page at several widths, and a chat node on the canvas.
- A real Claude Code turn in the installed window, asked a question whose best answer is a component,
  and the result looked at.
