//! Contract tests for the docs auditor.
//!
//! Both halves. Three must-NOT cases are real false positives measured on this repo's own docs, where
//! the first version produced 76 findings and every one was wrong.

use std::collections::HashSet;
use std::fs;
use std::path::Path;
use tempfile::TempDir;
use viby::checks::check_docs::audit_doc;

fn repo(files: &[(&str, &str)]) -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    for (rel, body) in files {
        let full = dir.path().join(rel);
        fs::create_dir_all(full.parent().unwrap()).unwrap();
        fs::write(full, body).unwrap();
    }
    dir
}

/// Every file under `d`, as the TypeScript test's own walk produced it (directories sorted by name).
fn index(root: &Path) -> Vec<String> {
    fn walk(d: &Path, out: &mut Vec<String>) {
        let mut entries: Vec<_> = fs::read_dir(d).unwrap().map(|e| e.unwrap()).collect();
        entries.sort_by_key(|e| e.file_name());
        for e in entries {
            let f = e.path();
            if e.file_type().unwrap().is_dir() {
                walk(&f, out);
            } else {
                out.push(f.to_string_lossy().into_owned());
            }
        }
    }
    let mut out = Vec::new();
    walk(root, &mut out);
    out
}

fn rules(root: &TempDir, doc: &str, scripts: &[&str]) -> Vec<String> {
    let text = fs::read_to_string(root.path().join(doc)).unwrap();
    let scripts: HashSet<String> = scripts.iter().map(|s| s.to_string()).collect();
    audit_doc(
        doc,
        &text,
        &root.path().to_string_lossy(),
        &scripts,
        &index(root.path()),
    )
    .into_iter()
    .map(|f| f.rule)
    .collect()
}

#[test]
fn a_documented_path_that_does_not_exist_is_p1() {
    // Needs one resolving reference so the root is trusted — otherwise the self-calibration correctly
    // reports root-unknown instead, which is the behaviour pinned in its own test below.
    let root = repo(&[
        (
            "README.md",
            "See `src/real.ts` and `src/gone.ts` for details.\n",
        ),
        ("src/real.ts", "x"),
    ]);
    assert!(rules(&root, "README.md", &[]).contains(&"stale-path".to_string()));
}

#[test]
fn a_path_that_exists_one_directory_deeper_still_resolves() {
    // 28 false findings on this repo came from guessing the base directory: a doc legitimately cites a
    // path relative to a package subdirectory. Resolution is by suffix against the real file index.
    let root = repo(&[
        ("README.md", "See `skills/brain/methods.md`.\n"),
        ("plugins/tk/skills/brain/methods.md", "x"),
    ]);
    assert_eq!(rules(&root, "README.md", &[]), Vec::<String>::new());
}

#[test]
fn english_prose_containing_make_is_not_a_script_invocation() {
    // The single worst false positive: `make` matched "make it readable", "make a decision" — 48 times.
    let root = repo(&[(
        "README.md",
        "We make it readable, then make a decision about the layout.\n",
    )]);
    assert_eq!(rules(&root, "README.md", &["check"]), Vec::<String>::new());
}

#[test]
fn a_regex_flag_in_backticks_is_not_a_path() {
    let root = repo(&[("README.md", "The pattern uses `/g` and `/i` flags.\n")]);
    assert_eq!(rules(&root, "README.md", &[]), Vec::<String>::new());
}

#[test]
fn a_documented_npm_script_that_does_not_exist_is_p1() {
    let root = repo(&[("README.md", "Run `npm run buidl` to build.\n")]);
    assert!(rules(&root, "README.md", &["build", "check"]).contains(&"unknown-script".to_string()));
}

#[test]
fn a_documented_script_that_does_exist_is_silent() {
    let root = repo(&[("README.md", "Run `npm run check` before pushing.\n")]);
    assert_eq!(rules(&root, "README.md", &["check"]), Vec::<String>::new());
}

#[test]
fn npm_install_npm_test_are_not_project_scripts() {
    let root = repo(&[("README.md", "Run `npm ci` then `npm test`.\n")]);
    assert_eq!(rules(&root, "README.md", &["check"]), Vec::<String>::new());
}

#[test]
fn a_dead_relative_link_is_p1_a_live_one_is_silent() {
    let root = repo(&[
        (
            "README.md",
            "See [guide](docs/guide.md) and [gone](docs/gone.md).\n",
        ),
        ("docs/guide.md", "# Guide\n"),
    ]);
    let f = rules(&root, "README.md", &[]);
    assert_eq!(
        f.iter().filter(|r| *r == "dead-link").count(),
        1,
        "{}",
        f.join(",")
    );
}

#[test]
fn a_dead_anchor_is_p2_and_a_live_anchor_is_silent() {
    let root = repo(&[
        (
            "README.md",
            "See [x](docs/g.md#the-real-heading) and [y](docs/g.md#renamed-away).\n",
        ),
        ("docs/g.md", "# Doc\n\n## The Real Heading\n\ntext\n"),
    ]);
    let f = rules(&root, "README.md", &[]);
    assert_eq!(
        f.iter().filter(|r| *r == "dead-anchor").count(),
        1,
        "{}",
        f.join(",")
    );
}

#[test]
fn placeholders_in_templates_are_not_references() {
    let root = repo(&[(
        "README.md",
        "Add `- [Title](file.md) — hook` to the index, in `path/to/your-file.ts`.\n",
    )]);
    assert_eq!(rules(&root, "README.md", &[]), Vec::<String>::new());
}

#[test]
fn when_nothing_resolves_the_root_is_reported_as_wrong_rather_than_the_doc_as_stale() {
    let root = repo(&[("README.md", "See `src/a.ts`, `src/b.ts` and `lib/c.ts`.\n")]);
    let f = rules(&root, "README.md", &[]);
    assert!(f.contains(&"root-unknown".to_string()), "{}", f.join(","));
    assert!(
        !f.contains(&"stale-path".to_string()),
        "must not claim staleness it could not check"
    );
}

#[test]
fn an_external_url_is_never_treated_as_a_path_or_a_link_to_resolve() {
    let root = repo(&[
        (
            "README.md",
            "See [the paper](https://arxiv.org/abs/1234.5678) and `https://x.dev/a.ts`.\n",
        ),
        ("a.ts", "x"),
    ]);
    assert_eq!(rules(&root, "README.md", &[]), Vec::<String>::new());
}

#[test]
fn a_one_segment_reference_does_not_resolve_against_an_unrelated_sibling_directory() {
    // Reviewer finding (P1): `scripts/helper.ts` ends the same way in every skill's own scripts dir, so
    // suffix matching reported a genuinely stale path as resolved — a silent false negative.
    let root = repo(&[
        (
            "skills/brain/README.md",
            "See `real/deep/module.ts` and `scripts/helper.ts`.\n",
        ),
        ("real/deep/module.ts", "x"),
        ("skills/docs/scripts/helper.ts", "x"),
    ]);
    assert!(
        rules(&root, "skills/brain/README.md", &[]).contains(&"stale-path".to_string()),
        "the sibling's file must not count"
    );
}

#[test]
fn a_repeated_headings_disambiguated_anchor_is_valid() {
    // Reviewer finding (P2): renderers expose the second `## Config` as `#config-1`.
    let root = repo(&[
        (
            "README.md",
            "See [a](docs/g.md#config) and [b](docs/g.md#config-1).\n",
        ),
        ("docs/g.md", "# Doc\n\n## Config\n\nx\n\n## Config\n\ny\n"),
    ]);
    assert_eq!(rules(&root, "README.md", &[]), Vec::<String>::new());
}
