# §5 in depth — refactoring in Mikado steps

Read while you are actually doing the conforming work, not while deciding whether to take the job on.

## 5. Refactor in Mikado steps, not in one heroic pass

Inherited code punishes plans made from the outside, so discover the order by trying:

1. Attempt the change you want, naively.
2. If it breaks something, **do not push through**. Write down what has to be true first — that is
   a prerequisite — and **revert**.
3. Do the prerequisites, deepest first, each landing green on its own.
4. Repeat. The graph you build *is* the plan.

The reverting feels wasteful and isn't: the discarded attempt bought you the dependency map, which
is the thing you actually lacked. It is also what produces the partition the fan-out law demands —
`/viby-toolkit:principles` §3 permits parallel writes only when you can name the partition and the
hub files, and a Mikado graph is exactly that naming. Take the hub nodes yourself, sequentially.

For a genuinely large adoption, prefer **strangle over rewrite**: stand the new implementation
beside the old, route traffic across piece by piece, and delete the old path only once nothing
calls it. A twelve-month rewrite abandoned at month eighteen is the default outcome otherwise.

