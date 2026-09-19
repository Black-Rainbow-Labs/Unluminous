# What Unluminous is

A text editor for macOS and Windows, written in Rust. It opens any file holding text, has a file
explorer with folders that expand in place, a terminal along the bottom with tabs, and it lets the
desktop show through its background while the text stays solid.

It is also an editor you can write code in — line numbers, a tab per file, syntax colouring from
twelve language plugins, git in full, a debugger, run configurations, completion, go to definition,
find all references and rename across the project — and an **AI-first IDE**, which is a claim with a
definition rather than a slogan.

## The claim

> **Everything a person can do in this window, an agent can do too, through the same command, and
> both are covered by automated tests.**

Not a plugin bolted on, not a subset of the interesting parts. `Ctrl/Cmd+Shift+O` and
`unluminous_modal open go-to-file` are one feature. A breakpoint set by clicking the gutter and one
set by `unluminous_debug breakpoint add` are the same breakpoint, in the same file, drawn the same
way. [For AI agents](for-ai-agents.md) is what makes that true rather than aspirational, and what it
cost.

## Who it is for

Somebody who writes code with an agent beside them, and who wants the agent and themselves to be
looking at the same window rather than at two copies of the same project.

That is a narrower audience than "people who edit text", and it is the one this is built for. The
consequences run through everything: a command exists for every menu entry because an agent cannot
click; a command answers in a payload proportionate to the question because an agent handed three
thousand tokens to learn one number stops asking; a read-only question never mutates the document
because an agent asking what a word would complete to must not type into somebody's file.

## The case for it against the editor you already use

There is not much of one on the ordinary things. VS Code, Zed, Sublime Text and the reference editor
all edit text well, and three of the four have been doing it for longer than this has existed. What is different is
three things.

**The agent drives the window you are looking at.** Every other arrangement gives the agent its own
copy of the project — its own file reads, its own writes, its own idea of what is open — and leaves
the two of you to reconcile. Here there is one window. A file the agent opens is open. A rename the
agent makes is one undo step you can undo. A breakpoint it sets is a red dot you can see.

**What the agent is given is generated from what the product can do**, so it cannot fall behind. A
command added to Unluminous is a tool the day it is added, with its summary, its arguments and its
flags, and a test fails if one ever is not. There is no second list to keep in step, and no version of
the documentation that is a month out of date.

**The desktop shows through it.** That is not a feature so much as the character of the thing, and it
is the reason several decisions went the way they did — a colour scheme colours the tokens and not
the editing area, because a scheme that repainted the background opaque would trade it away to be a
shade nearer a screenshot. [What it looks like](overview.md) is thirty-seven pictures of that.

## What it is not

It is not a language server client, and go to definition, find all references and rename are a
**syntactic** index built from the token stream rather than a semantic one. Where the mechanism cannot
tell two same-named things apart it shows both rather than guessing one, and a definition found by a
heuristic is marked as one all the way to the screen.
[Not included](not-included.md) is the whole list of what is deliberately absent, with the reason for
each.

It fetches nothing, ever. Not a plugin, not a debug adapter, not a picture named in a Markdown file
with a scheme in front of it, not telemetry. A package manager somebody pressed a button for is not
the editor reaching out, and neither is an agent they pressed send on; everything else is off.

## Where to go next

| | |
|---|---|
| [Getting started](getting-started.md) | install it, run it, the switches, windows and projects |
| [What it looks like](overview.md) | thirty-seven pictures of the running window |
| [The window](the-window.md) | every part of it named, the nine menus, every Settings page |
| [For AI agents](for-ai-agents.md) | the contract, how it is enforced, and how to install it into one |
| [Architecture](architecture.md) | eight crates, one frame, the seams, the threads |
