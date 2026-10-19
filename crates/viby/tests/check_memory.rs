//! Contract tests for the memory auditor.
//!
//! Both halves. Two of the must-NOT cases are real false positives this checker produced on the first
//! run against five real memory stores — it reported 23 stale references, almost all of them simply
//! resolved against the wrong project root.

use std::fs;
use std::path::Path;

use viby::checks::check_memory::{audit_entry, audit_store};

fn good() -> String {
    [
        "---",
        "name: build-needs-node-22",
        "---",
        "",
        "The build fails on Node 20 because type stripping landed in 22.6. Verified 2026-07-29 by running",
        "`npm run check` on both. Outcome: pinning the engine field fixed it.",
        "",
    ]
    .join("\n")
}

fn tmp() -> String {
    std::env::temp_dir().to_string_lossy().into_owned()
}

fn path_str(p: &Path) -> &str {
    p.to_str().expect("utf8 path")
}

fn store(files: &[(&str, String)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    for (n, body) in files {
        fs::write(dir.path().join(n), body).expect("write entry");
    }
    dir
}

fn checks_of(dir: &tempfile::TempDir) -> Vec<&'static str> {
    audit_store(path_str(dir.path()), &tmp())
        .expect("store")
        .findings
        .iter()
        .map(|f| f.check)
        .collect()
}

#[test]
fn a_well_formed_entry_produces_no_findings() {
    let got: Vec<&str> = audit_entry("a.md", &good(), &tmp())
        .iter()
        .map(|f| f.check)
        .collect();
    assert!(got.is_empty(), "{got:?}");
}

#[test]
fn an_undated_entry_is_flagged_it_can_never_be_retired_on_evidence() {
    let f = audit_entry(
        "a.md",
        "The build needs Node 22 because type stripping landed there.\n",
        &tmp(),
    );
    assert!(f.iter().any(|x| x.check == "undated"));
}

#[test]
fn an_entry_with_no_provenance_is_flagged() {
    let f: Vec<&str> = audit_entry("a.md", "Deploys happen on Fridays. 2026-07-29\n", &tmp())
        .iter()
        .map(|x| x.check)
        .collect();
    assert!(f.contains(&"no-provenance"), "{f:?}");
}

#[test]
fn an_entry_recording_hearsay_as_fact_is_flagged_as_unverified() {
    let text = "2026-07-29 — the user said the API is rate limited, so avoid retries.\n";
    assert!(audit_entry("a.md", text, &tmp())
        .iter()
        .any(|x| x.check == "unverified-claim"));
}

#[test]
fn a_stale_path_reference_is_p1() {
    let dir = tempfile::tempdir().expect("tempdir");
    fs::create_dir_all(dir.path().join("src")).expect("mkdir");
    fs::write(
        dir.path().join("src").join("real.ts"),
        "export const a = 1;\n",
    )
    .expect("write");
    // One reference resolves, so the root is trusted; the other is genuinely gone.
    // (A reference needs a slash to count — a bare filename is too ambiguous to resolve.)
    let text = "2026-07-29, verified: see `src/real.ts` and `src/deleted.ts`.\n";
    let f = audit_entry("a.md", text, path_str(dir.path()));
    let hit = f
        .iter()
        .find(|x| x.check == "stale-reference")
        .unwrap_or_else(|| panic!("no stale-reference in {f:?}"));
    assert_eq!(hit.severity, "P1");
    assert!(hit.problem.contains("deleted.ts"));
}

#[test]
fn when_nothing_resolves_the_root_is_wrong_say_so_instead_of_inventing_stale_findings() {
    // The real false positive: five stores audited against one repo root produced 23 "stale"
    // references, none of them stale. A comparison that could not happen must not read as a finding.
    let text = "2026-07-29, verified: see `src/a.ts` and `src/b.ts` and `lib/c.ts`.\n";
    let root = Path::new(&tmp()).join("no-such-root");
    let f: Vec<&str> = audit_entry("a.md", text, path_str(&root))
        .iter()
        .map(|x| x.check)
        .collect();
    assert!(f.contains(&"root-unknown"), "{f:?}");
    assert!(
        !f.contains(&"stale-reference"),
        "must not claim staleness it could not check"
    );
}

#[test]
fn a_path_that_resolves_against_an_ancestor_of_the_root_is_not_stale() {
    // A memory about a project legitimately cites repo-root paths, package paths and home paths.
    let root = tempfile::tempdir().expect("tempdir");
    fs::create_dir_all(root.path().join("pkg")).expect("mkdir");
    fs::write(root.path().join("top.md"), "x").expect("write");
    fs::create_dir_all(root.path().join("docs")).expect("mkdir");
    fs::write(root.path().join("docs").join("top.md"), "x").expect("write");
    let pkg = root.path().join("pkg");
    let f = audit_entry(
        "a.md",
        "2026-07-29, verified `docs/top.md`.\n",
        path_str(&pkg),
    );
    assert!(!f.iter().any(|x| x.check == "stale-reference"), "{f:?}");
}

#[test]
fn placeholders_in_examples_are_not_treated_as_references() {
    let f = audit_entry(
        "a.md",
        "2026-07-29, verified. Put it in `path/to/your-file.ts`.\n",
        &tmp(),
    );
    assert!(!f.iter().any(|x| x.check == "stale-reference"));
}

#[test]
fn an_entry_that_grew_into_a_document_is_flagged() {
    let long = good() + "\n" + &"word ".repeat(1000);
    assert!(audit_entry("a.md", &long, &tmp())
        .iter()
        .any(|x| x.check == "oversized-entry"));
}

#[test]
fn an_entry_missing_from_the_index_is_p1_retrieval_failure_is_where_memory_errors_are() {
    let dir = store(&[
        ("MEMORY.md", "# Index\n\n- [a](a.md) — the thing\n".into()),
        ("a.md", good()),
        ("b.md", good()),
    ]);
    let f = audit_store(path_str(dir.path()), &tmp())
        .expect("store")
        .findings;
    let hit = f
        .iter()
        .find(|x| x.check == "not-indexed")
        .unwrap_or_else(|| {
            panic!(
                "no not-indexed in {:?}",
                f.iter().map(|x| x.check).collect::<Vec<_>>()
            )
        });
    assert_eq!(hit.severity, "P1");
    assert_eq!(hit.file, "b.md");
}

#[test]
fn an_index_link_pointing_at_nothing_is_p1() {
    let dir = store(&[
        (
            "MEMORY.md",
            "- [gone](gone.md) — hook\n- [a](a.md) — hook\n".into(),
        ),
        ("a.md", good()),
    ]);
    assert!(checks_of(&dir).contains(&"index-dangling"));
}

#[test]
fn a_store_with_entries_and_no_index_at_all_is_p1() {
    let dir = store(&[("a.md", good())]);
    assert!(checks_of(&dir).contains(&"no-index"));
}

#[test]
fn near_duplicate_topics_are_flagged_for_merging() {
    let dir = store(&[
        (
            "MEMORY.md",
            "- [a](build-needs-node-version.md)\n- [b](build-needs-node-engine.md)\n".into(),
        ),
        ("build-needs-node-version.md", good()),
        ("build-needs-node-engine.md", good()),
    ]);
    assert!(checks_of(&dir).contains(&"duplicate-topic"));
}

#[test]
fn distinct_topics_are_not_flagged_as_duplicates() {
    let dir = store(&[
        (
            "MEMORY.md",
            "- [a](build-needs-node-22.md)\n- [b](deploy-window-fridays.md)\n".into(),
        ),
        ("build-needs-node-22.md", good()),
        ("deploy-window-fridays.md", good()),
    ]);
    let f = checks_of(&dir);
    assert!(!f.contains(&"duplicate-topic"), "{f:?}");
}

#[test]
fn an_empty_store_is_nothing_to_check_not_clean() {
    let dir = store(&[]);
    assert_eq!(
        audit_store(path_str(dir.path()), &tmp())
            .expect("store")
            .entries,
        0
    );
}

#[test]
fn same_project_notes_on_the_same_day_are_not_duplicates() {
    // Measured on a real store: under a `project_<name>_<topic>_<date>` convention the shared project
    // and date tokens dominated the comparison, so two genuinely different notes looked identical.
    let dir = store(&[
        (
            "MEMORY.md",
            "- [a](project_acme_audit_2026-04-27.md)\n- [b](project_acme_qa_discovery_2026-04-27.md)\n"
                .into(),
        ),
        ("project_acme_audit_2026-04-27.md", good()),
        ("project_acme_qa_discovery_2026-04-27.md", good()),
    ]);
    let f = checks_of(&dir);
    assert!(
        !f.contains(&"duplicate-topic"),
        "an audit and a QA discovery are different notes: {f:?}"
    );
}

#[test]
fn but_genuinely_duplicated_topics_are_still_caught() {
    let dir = store(&[
        (
            "MEMORY.md",
            "- [a](project_acme_deploy_window_2026-04-27.md)\n- [b](project_acme_deploy_schedule_window.md)\n"
                .into(),
        ),
        ("project_acme_deploy_window_2026-04-27.md", good()),
        ("project_acme_deploy_schedule_window.md", good()),
    ]);
    assert!(checks_of(&dir).contains(&"duplicate-topic"));
}

#[test]
fn a_single_token_filename_is_not_a_duplicate_of_a_longer_specific_one() {
    // Reviewer finding (P2): min() made `auth.md` score 1.0 against `auth-token-refresh-race.md`.
    let dir = store(&[
        (
            "MEMORY.md",
            "- [a](auth.md)\n- [b](auth-token-refresh-race.md)\n".into(),
        ),
        ("auth.md", good()),
        ("auth-token-refresh-race.md", good()),
    ]);
    assert!(!checks_of(&dir).contains(&"duplicate-topic"));
}

#[test]
fn a_project_path_is_not_reported_stale_just_because_a_sibling_memory_file_resolved() {
    // Found by running this checker on the author's own memory store: the entry cited
    // `viby-toolkit-modules.md` (a real sibling) AND `plugins/viby-toolkit/lib/strip-noncode.ts` (real,
    // but in a repo this checker was never given). One resolving sibling made the root "look right", so
    // the project path was reported as a P1 stale-reference — a confidently wrong finding about a file
    // that exists. Two namespaces, one root assumption.
    let dir = store(&[
        ("MEMORY.md", "- [a](a.md) — hook\n".into()),
        ("a.md", "See `other.md` and `src/deep/module.ts`.\n".into()),
        ("other.md", "x\n".into()),
    ]);
    let text = fs::read_to_string(dir.path().join("a.md")).expect("read");
    let rules: Vec<&str> = audit_entry("a.md", &text, path_str(dir.path()))
        .iter()
        .map(|f| f.check)
        .collect();
    assert!(
        !rules.contains(&"stale-reference"),
        "must not claim stale, got {rules:?}"
    );
}
