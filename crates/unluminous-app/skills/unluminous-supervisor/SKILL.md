---
name: unluminous-supervisor
description: Supervise the Unluminous Agent-Tasks board, working the QA Failed lane first and then New, one ticket at a time, each in an agent of its own, until both lanes are empty. Use when the person says "process the board", "work the tickets", "run the supervisor", or when you were started as the board's supervisor by `Chat with supervisor`.
---

# Supervising the Agent-Tasks board

You do not work tickets yourself. You choose the next ticket, start an agent on it, watch it, answer what it
cannot answer, and check its hand back before you move on. The ticket's own agent follows the
`unluminous-task` skill.

Every command here is a verb of the board, reached through the `unluminous_agent_tasks` MCP tool
(`{"command": "board"}`) or through `unluminous-cli plugins run agent-tasks <verb> <arguments...>` in a
terminal. They are the same commands.

## The loop

1. **Read the board.** `board` answers the lanes and their cards. Work `qa_failed` before `new`, and within a
   lane take the highest priority first, then the oldest.
2. **Read the ticket before starting it.** `task task-12`. For a QA Failed ticket, the newest comment from a
   person is the reason it came back; make sure the agent will see it.
3. **Start its agent.** `start task-12` launches the agent the ticket is assigned to, in the ticket's project
   folder, and types the hand over line into it. `assign task-12 <endpoint>` first when it has nobody.
4. **Watch it.** Every few minutes read `terminal task-12 40` and `chat task-12 10`. If the agent stops on a
   menu, `choice task-12` shows the options and `choice task-12 <n>` answers one. If it asks a question you can
   answer from the ticket, answer it with `send task-12 <words>`. If only the person can answer, post the
   question with `comment task-12 <question>` and go on to the next ticket.
5. **Check the hand back.** When the ticket reaches `agent_done`, read its last comment. It should say what
   changed and how it was checked. If it does not, or a todo is still open, `comment-send task-12 <what is
   missing>` sends it back to the agent.
6. **Go round again** until `qa_failed` and `new` are empty, then say what was done in one comment per ticket
   you touched and stop.

## Rules

- **One ticket at a time in one project folder**, unless the tickets plainly touch different parts of it.
- **Never move a ticket to `agent_done` yourself.** That is the ticket agent's statement that it checked its own
  work.
- **Never start a ticket in the `wait` lane** or one assigned to `human`.
- **A ticket that cannot go on** is moved back to `new` with a comment that names what is in the way, so it does
  not sit in `in_progress` with nobody on it.
- `interrupt task-12` stops an agent that is doing something wrong; `escape task-12` stops only its current turn.
