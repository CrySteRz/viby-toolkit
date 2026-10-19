# Brief for every Rust port task (T3–T11)

Read this before writing code. The plan is `docs/plans/2026-10-19-rust-port.md`.

## The job

You port one or more TypeScript files to Rust in `crates/viby`. **The TypeScript is the oracle:**
the Rust must give the same output (plain text and `--json`) and the same exit codes for the same
inputs. The TypeScript stays in the repo until the whole port is done, so you can run it at any
time:

```sh
sh plugins/viby-toolkit/hooks/run.sh plugins/viby-toolkit/skills/<skill>/scripts/<name>.ts [args]
cargo run -q -p viby -- <name> [args]          # the Rust subcommand of the same name
```

## Rules

- **Touch only the files your task owns** (listed in your prompt). Do not edit `Cargo.toml`,
  `src/lib.rs`, `src/main.rs`, any `mod.rs`, or `src/core/*`. These are already wired: your
  module exists as a stub with `pub fn main(args: &[String]) -> i32`, and dispatch calls it.
  If you need something that is not there, stop and say so in your report; do not work around it.
- **Available crates:** `serde` (derive), `serde_json` (with `preserve_order`), `regex`,
  `fancy-regex`, `base64`, `agentgrep` (v0.1.7), and `windows-sys` (Windows only, with Pipes,
  FileSystem, IO, Security, Threading and Foundation). Dev-only: `tempfile`. Nothing else.
- **Shared helpers:**
  - `viby::core::args`
    - Strict `parseArgs` semantics.
    - `parse_or_exit(args, &[flag("json"), flag("quiet"), value("base", Some("HEAD")), values("against")])`
      returns `Parsed`. Call `.flag()`, `.value()`, `.values()` and read `.positionals`.
    - Unknown option means exit 1.
  - `viby::core::git`
    - `git(cwd, &[..])` and `git_within(cwd, &[..], Duration)`.
    - Each returns `Output { ok, stdout, stderr }`.
  - `viby::core::strip::strip_noncode(text, ".ext")`
    - The extension takes a leading dot and must be lowercase.
    - **Byte-preserving:** each blanked character becomes as many `\0` bytes as it has in UTF-8.
      A byte offset in the output is therefore the same byte offset in the input.
    - `regex_can_start_here` and `is_js_space` are also exported.
- **Keep the module shape:** a `pub fn main(args: &[String]) -> i32` plus the `pub` functions
  the tests need, mirroring the TS exports (`checkPlan` becomes `check_plan`, and so on). Keep the
  TS file's header comment and its "why" comments; they record past bugs.
- No `unwrap()` on input data. A bad file is a finding or a skip, as in the TS, never a panic.
- `cargo clippy --all-targets -- -D warnings` must be clean, and `cargo fmt`.

## JavaScript to Rust traps

- **Regex:**
  - The `regex` crate has no lookaround and no backreferences. Use `fancy_regex` only for those
    patterns.
  - JS `\w`, `\d` and `\b` are ASCII. Rust's are Unicode. Write `[A-Za-z0-9_]` and `[0-9]`, and
    use `(?-u:\b)`, to match the JS.
  - JS `/m` is `(?m)` and `/i` is `(?i)`. JS `\s` includes `\u{feff}`.
  - A `/g` `exec` loop is `captures_iter`.
- **Positions:**
  - Lines are 1-based.
  - When the TS printed a column, it was a UTF-16 index. Compute it as
    `text[line_start..at].encode_utf16().count()`, plus 1 if the TS added 1.
  - `text.length` in JS counts UTF-16 units. Where a length is shown or compared against a limit,
    use `encode_utf16().count()`.
- **Strings:**
  - `split("\n")` keeps a trailing empty element, and so does Rust's `split('\n')`. Keep it.
  - `trim()` in JS also trims `\u{feff}`.
- **JSON:**
  - `JSON.stringify(x, null, 2)` is `serde_json::to_string_pretty`. Build objects with `json!` in
    the same key order.
  - JS omits `undefined` fields: omit them too.
  - JS prints `1`, not `1.0`, for a whole number. Use integers where the TS value is whole.
    Check every float field against the TS output.
- **Sorting:** JS `sort` is stable, and so is `sort_by`. Compare the same keys in the same order.
  `localeCompare` is not byte order: check with the TS on mixed-case input.
- **File walking:** `readdirSync` order is OS order. If the TS sorted, sort. If it did not, sort
  anyway and say so in your report (a deterministic order is an intended difference).

## Tests

- Port every `test(...)` in your task's `tests/<name>.test.ts` to `crates/viby/tests/<file>.rs`
  (the file names are in the plan).
- Keep the test name, as snake_case, and the same assertions. Do not weaken one, merge two, or
  skip one.
- The Rust test count must be at least the TS count.
- A TS test that spawned the script spawns `env!("CARGO_BIN_EXE_viby")` with the subcommand.
- A TS test that called an exported function calls the `pub` Rust function.
- Fixtures the TS built in a temporary folder are built with `tempfile::tempdir()`.
- A test that depends on a tool (git, sh) keeps the same skip behaviour as the TS.

## Parity, before you report

1. Run the TS and the Rust on the same inputs, both plain and `--json`, and diff the outputs.
   - Inputs: the repo itself (`.`, or `--all` where the script takes it), every fixture your
     tests build, and at least one other real repo under
     `/Users/ionutblidaru/Projects/Personal/viby-ecosystem/` (viby, viby-flow, viby-shot). All of
     these are read-only for you.
   - Compare `--json` with `jq -S .`, and plain output byte for byte.
2. The bar is zero differences. Fix each one, or list it as intended with a one-line reason
   (deterministic order, or a TS bug the TS comments already call a bug).

## Commit and report

- Commit on your branch: `rust port T<n>: <what>`. Never add a co-author or "Generated with"
  line.
- Report in at most 30 lines:
  - your files, and the test counts for TS and Rust
  - the `cargo test` command and its result line
  - each parity command with its result (the diff line count)
  - every intended difference
  - anything you could not do
