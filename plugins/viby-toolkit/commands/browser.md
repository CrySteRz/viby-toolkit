---
description: Set up browser control (Brave, Chrome, Edge or Firefox) and prove it works — guided, one time, about 20 seconds.
argument-hint: [optional: a page to open once connected]
---

Get the viby-toolkit `browser` tool connected to the person's own browser, and show it
working. Speak to the person in plain words; they may never have done this before.

1. Call the `browser` tool with `action='status'`.
   - If it says **Ready**, skip to step 4.
2. If it is not ready, the tool has already opened the right page for the person and put the
   path they need on their clipboard (or, with no usable browser, opened a download page).
   Show the steps it returned exactly as written, as a numbered list, and end your reply.
3. When they say it is done, call `action='status'`.
   - **Ready**: say so in one line.
   - **Not yet**: show the new steps it returns (they change with what it finds, for example
     "switch it on") and wait again. Stop after three rounds and say what is still missing.
4. Prove it: `action='open'` with $ARGUMENTS if a page was given, otherwise
   `https://example.com`, then `action='snapshot'`. Show the person the page title and say
   that Claude now works in its own background tab, so their current tab is left alone.
5. In two lines, tell them how to use it from here: ask for anything in the browser, and
   for a longer task Claude hands it to the browser agent. Mention that passwords, codes and
   payments always stay with them.

Never ask the person to run a terminal command for this, and never do the Developer-mode
step on their behalf.
