---
name: api
description: >
  Always load before designing or changing an interface others depend on — an endpoint, a public
  surface, an event schema, a CLI, an exported type. "design an API", "add an endpoint", "review
  this interface", "is this a breaking change". Not /viby-toolkit:schema.

---

# API (design the contract, because you cannot take it back)

```
IRON LAW: Design from the CALLER's use case inward, never from the storage schema outward.
          Every field is a promise you must keep, so add the fewest that satisfy the use case.
          Adding is cheap; changing and removing are not. Decide errors, pagination and
          idempotency at design time — retrofitting them is a breaking change.
```

Follow `/viby-toolkit:principles`. This is a doctrine skill: it encodes established interface
design practice, not a research finding, and is written that way.

An interface is the one artifact you cannot refactor freely, because its consumers are not in
your repository. That is why the work goes in before implementation: internals are cheap to
change later and the surface is not.

## 1. Write the caller's code first

Before designing anything, write the two or three calls a consumer will actually make — as
code, in their idiom. This surfaces awkwardness that a schema never will: a required field
they cannot know yet, two calls that must always be made together, a response they have to
post-process before it is useful.

Then ask what the caller genuinely needs to *do*, and design the smallest surface that lets
them do it. Do not expose your storage shape: today's table becomes tomorrow's two tables,
and if the wire format mirrored the schema, that refactor became a breaking change.

## 2. Make the shape predictable

- **Nouns and consistent verbs**; the same concept named the same way everywhere. Two names
  for one thing costs every reader forever.
- **Consistent casing, consistent plurals, consistent date format** (ISO-8601, UTC). Small
  inconsistencies are what consumers hit first and remember longest.
- **Return objects, not bare arrays or scalars**, at any boundary you may need to extend —
  `{items: [...], next: "..."}` can gain a field; a top-level array cannot.
- **Make optionality explicit.** Distinguish absent from null from empty, and document which
  you mean; consumers *will* rely on the distinction whether or not you intended it.
- **Prefer explicit over clever.** An enum with four documented values beats a boolean that
  later needs a third state, and beats a free-form string that becomes an undocumented enum.

## 3. Decide the hard parts now, not later

Each of these is a breaking change if you add it after the fact, which is why they belong in
the design and not the backlog:

- **Errors are part of the contract.** Define the shape (code, message, and a stable
  machine-readable identifier), which conditions produce which, and what is retryable.
  Consumers branch on these; a changed error code breaks them as surely as a removed field.
  Never leak internals — a stack trace or SQL string in an error body is both a poor contract
  and a security finding (`/viby-toolkit:secure`).
- **Pagination**, on anything that returns a list. Collections always grow. Cursors survive
  inserts; offsets silently skip and duplicate rows under concurrent writes.
- **Idempotency**, on anything that mutates. Networks retry whether or not you planned for it:
  accept an idempotency key, or make the operation naturally repeatable, or you will
  eventually charge someone twice.
- **Limits** — page size, payload size, rate. An unbounded input is a denial-of-service
  surface, and adding a limit later breaks whoever was exceeding it.
- **Versioning strategy**, decided before v1 ships: how a breaking change will be delivered
  (path or header version, a parallel field, a new endpoint) and how long the old shape lives.

## 4. Design for compatible evolution

Assume you will be wrong about something. Make being wrong cheap:

- **Additive changes are safe; everything else needs a migration path.** New optional field,
  new endpoint, new enum value *if consumers were told to tolerate unknowns* — say so in the
  contract, or adding one is breaking.
- **Never repurpose a name.** Changing what a field means while keeping its name is the worst
  kind of breaking change, because nothing fails loudly — it just becomes quietly wrong.
- **Expand then contract**, exactly as in `/viby-toolkit:schema`: add the new shape, support both,
  migrate consumers, remove the old one a release later. Deprecate in the response and the
  docs before you remove, and be able to *see* whether anyone still calls it.
- Consult `/viby-toolkit:release` when the change lands: the version number is the promise you
  are making about all of this, and a break hidden in a patch is the failure mode there.

## 5. Verify the contract, don't just describe it

- **Write the contract down** in whatever form the stack supports (OpenAPI, a schema file,
  exported types) and generate or validate against it, so the docs cannot drift from reality.
- **Test it as a consumer would**, from outside, including the error cases and the limits —
  the internals passing their unit tests says nothing about the surface being right.
- **Diff the surface** before and after a change. That diff, not the size of the diff in the
  implementation, is what tells you whether this is breaking — and it is mechanical, so run it:

  ```bash
  SURFACE=$(ls "$HOME"/.claude/plugins/cache/*/viby-toolkit/*/skills/api/scripts/check-api-surface.ts 2>/dev/null | tail -1)
  RUN=$(ls "$HOME"/.claude/plugins/cache/*/viby-toolkit/*/hooks/run.sh 2>/dev/null | tail -1)
  sh "$RUN" "$SURFACE" --base main src        # added / removed / re-signatured exports
  ```

  Treat a clean result as *no syntactic break found*, not as *compatible*: the defaults,
  nullability, ordering and error-contract changes in step 4 are invisible to it, and those are
  where the quiet breaks live.

## Output

- The caller's use case, and the example calls you wrote first.
- The surface: endpoints/exports, shapes, and the error contract.
- The four decisions from step 3, stated explicitly even where the answer is "not needed here,
  because…".
- Compatibility assessment: additive, or breaking with a migration path.
- Verification: the contract artifact and the consumer-side test.
