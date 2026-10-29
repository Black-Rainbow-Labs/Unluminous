---
name: unluminous-task
description: Work one ticket on the Unluminous Agent-Tasks board from start to finish. Use when the person says "begin task-N", "work task-N", "pick up the next ticket", or when a line typed into your terminal by the board names a ticket key such as task-12. Reads the ticket, plans it as todos, does the work, comments on what was done and how it was checked, and hands it back in the Agent Done lane.
---

# Working a ticket on the Agent-Tasks board

The Agent-Tasks board lives in the Unluminous window. Every change to a ticket goes through the board's own
commands, so the person watching the board sees it happen. There are two ways to reach them, and they are the
same commands:

- **MCP**: the `unluminous_agent_tasks` tool (your client puts its own prefix in front), with the verb in
  `command` and the rest in `arguments`: `{"command": "task", "arguments": {"words": "task-12"}}`.
- **Shell**: `unluminous-cli plugins run agent-tasks <verb> <arguments...>`. `unluminous-cli` is on the `PATH`
  of every terminal Unluminous starts, and `UNLUMINOUS_INSTANCE` already says which window.

A ticket is named by its key, such as `task-12`. A todo is named by its number in the list, counted from 1.

## The steps, in order

1. **Read the ticket.** `task task-12` answers its title, description, lane, todos and comments. The newest
   comment from a person is your most important instruction: if the ticket has been worked before, that comment
   says what is still wrong.
2. **Claim it.** `move-task task-12 in_progress`. Skip this when it is already in progress.
3. **Plan it as todos.** One `todo-add task-12 <text>` for each concrete step, three to eight of them. When the
   ticket already has todos, carry on with the ones not ticked instead of writing a second plan.
4. **Do the work.** Tick each todo as it is finished, not all at the end: `todo-done task-12 3`.
5. **Say how it is going on long work.** Post `comment task-12 <what is done, what is next>` about every twenty
   minutes, and `heartbeat task-12 60` before a step you expect to take an hour, so the board can tell a slow
   agent from a stopped one.
6. **Check it.** Run what you built, the way the ticket would be judged: the program, the page, the command, the
   tests that cover it. A change nobody ran is not finished.
7. **Report.** `comment task-12 <what changed, the decisions, and how you checked it>`.
8. **Read the ticket again.** The person may have added a comment or a todo while you worked. Do that too, then
   read it again.
9. **Hand it back.** `move-task task-12 agent_done`. The board refuses while a todo is not ticked; tick it, or
   take it out with `todo-remove` when it no longer applies. Never use `anyway` to get round the refusal.

## When you cannot finish

Post a comment that says exactly what is finished, what is left, and what is in the way, then move the ticket
back with `move-task task-12 new`. Never leave a ticket in `in_progress` that nobody is working on: the board
reads that as an agent at work.

## Lanes

| Lane | Meaning |
|---|---|
| `new` | Waiting to be picked up. |
| `qa_failed` | Finished once and sent back by a person; their newest comment says why. Fix that first. |
| `in_progress` | An agent is on it now. |
| `agent_done` | Finished and checked by the agent, waiting for a person. |

## Useful verbs

| Verb | What it does |
|---|---|
| `task <key>` | The ticket, with its todos and comments. |
| `search <words>` | Tickets whose key, title or description holds the words. |
| `todo-add <key> <text>` / `todo-done <key> <n>` / `todo-remove <key> <n>` | The plan. |
| `comment <key> <text>` | A comment, as markdown. |
| `describe <key> <text>` | Replace the description. |
| `move-task <key> <lane>` | Change the lane. |
| `heartbeat <key> <minutes>` | Say you are still working. |
| `new-task <title>` | File a follow up ticket, which answers its key. |
