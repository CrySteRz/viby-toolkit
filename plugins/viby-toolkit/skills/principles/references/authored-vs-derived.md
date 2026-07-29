# §9 in depth — authored vs derived artifacts

Read before writing any map, index, cache, or generated file, and before trusting one you find.

Work produces two kinds of artifact, and treating one as the other is how a session ends up
confidently reasoning from a stale map.

- **Authored** artifacts are the *why* — decisions, lessons, requirements, contracts, plans
  (`learn`, `plan`, `api`): written with the user, reviewed, durable, the source of truth.
- **Derived** artifacts are the *what* — maps, indexes, scan output, caches, generated clients:
  rebuilt from the code by a command, disposable, and never a source of truth.

Keep them layered rather than merged, so neither has to pretend to be the other. They connect by
stable reference (`file:line`, an ID), never by copying each other's content — **a copy is what goes
silently stale.**

## The three rules

- **Stamp a derived artifact with its provenance** — the commit or date it was built from, and the
  command that rebuilds it. Without those, staleness is invisible and the thing can only be believed,
  not checked. An `explore` map is derived: stamp it. When derived disagrees with its source, **the
  source wins** — regenerate rather than hand-patch, because patching quietly converts it into an
  authored file no command can reproduce.
- **Don't commit what a command can rebuild**, unless rebuilding is expensive. Commit the *guidance*
  on when to trust it and when to verify it — that is the genuinely authored part.
- **A heuristic derived artifact is a planning aid, not evidence.** Fine for "where should I look";
  never the proof in an evidence-gated claim (§5), and never the basis for a change to auth, payments
  or migrations without verifying the specific edge you relied on.
