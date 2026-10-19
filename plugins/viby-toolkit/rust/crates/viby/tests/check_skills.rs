//! Contract tests for the skill-library health check.
//!
//! The first version of this checker used thresholds picked by feel (0.38/0.50) against a
//! metric whose real-world maximum is ~0.13 — so it could never fire and reported "clean"
//! forever. These tests exist mainly to pin that it is NOT vacuous: a deliberately shadowed
//! pair must be caught, and a genuinely distinct pair must not be.
//!
//! Port of `tests/check-skills.test.ts`.

use std::fs;
use std::path::PathBuf;

use viby::checks::check_skills::{
    check_listing_budget, check_progressive_disclosure, check_skills, listing_budget_chars,
    load_skills, Skill, DEFAULT_CONTEXT_TOKENS,
};

fn library(skills: &[(&str, &str)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for (name, frontmatter) in skills {
        let d = dir.path().join(name);
        fs::create_dir_all(&d).unwrap();
        fs::write(
            d.join("SKILL.md"),
            format!("---\n{frontmatter}\n---\n\n# {name}\n\nbody\n"),
        )
        .unwrap();
    }
    dir
}

fn path_str(dir: &tempfile::TempDir) -> String {
    dir.path().to_string_lossy().into_owned()
}

/// `(check, skills)` for every finding in a library built from `skills`.
fn findings(skills: &[(&str, &str)]) -> Vec<(String, Vec<String>)> {
    let dir = library(skills);
    check_skills(&path_str(&dir))
        .findings
        .into_iter()
        .map(|f| (f.check, f.skills))
        .collect()
}

fn debug(f: &[(String, Vec<String>)]) -> String {
    format!("{f:?}")
}

fn skill(name: &str, description: &str) -> Skill {
    Skill {
        name: name.to_string(),
        description: description.to_string(),
        triggers: vec![],
        model_invocable: true,
        directives: 0,
        body_lines: 1,
        body_words: 1,
        has_references: false,
    }
}

#[test]
fn not_vacuous_a_deliberately_shadowed_pair_is_caught() {
    let f = findings(&[
        ("alpha", "name: alpha\ndescription: Use when reviewing a diff before shipping, checking correctness, finding bugs, and judging whether the change is safe to merge."),
        ("beta", "name: beta\ndescription: Use when reviewing a diff before merging, checking correctness, finding bugs, and judging whether the change is safe to ship."),
    ]);
    let shadow: Vec<_> = f.iter().filter(|x| x.0 == "shadowing").collect();
    assert_eq!(
        shadow.len(),
        1,
        "expected a shadowing finding, got {}",
        debug(&f)
    );
    let mut names = shadow[0].1.clone();
    names.sort();
    assert_eq!(names, vec!["alpha", "beta"]);
}

#[test]
fn genuinely_distinct_skills_are_not_flagged() {
    let f = findings(&[
        ("perf", "name: perf\ndescription: Use for speed, memory or cost — profiling a bottleneck and measuring a before and after number."),
        ("docs", "name: docs\ndescription: Use when writing user-facing documentation, tutorials, or reference pages for an audience outside the team."),
    ]);
    assert!(
        f.is_empty(),
        "distinct skills must be silent, got {}",
        debug(&f)
    );
}

#[test]
fn a_trigger_phrase_claimed_by_two_skills_is_a_p1_collision() {
    let f = findings(&[
        ("one", "name: one\ndescription: Use for auditing dependency licences. Trigger when the user says \"check the licences\" or asks about legal compliance."),
        ("two", "name: two\ndescription: Use for scanning container images for outdated packages. Trigger when the user says \"check the licences\" before a release build."),
    ]);
    let dup = f.iter().find(|x| x.0 == "duplicate-trigger");
    let dup = dup.unwrap_or_else(|| panic!("expected duplicate-trigger, got {}", debug(&f)));
    let mut names = dup.1.clone();
    names.sort();
    assert_eq!(names, vec!["one", "two"]);
}

#[test]
fn a_mutually_cross_referencing_pair_is_treated_as_already_disambiguated() {
    // Adding "distinct from X" RAISES word overlap while LOWERING real confusion. Scoring it
    // as shadowing would penalise the fix and reward vagueness.
    let f = findings(&[
        ("migrate", "name: migrate\ndescription: Use for a wide mechanical sweep across many files, a rename everywhere or a codemod. Distinct from refactor, which restructures one area without changing behaviour."),
        ("refactor", "name: refactor\ndescription: Use when restructuring one area without changing behaviour, extracting a function or reducing duplication. Distinct from migrate, which is a wide mechanical sweep across many files."),
    ]);
    assert!(
        !f.iter()
            .any(|x| x.0 == "shadowing" || x.0 == "shadowing-watch"),
        "a mutually-disambiguated pair must not be flagged, got {}",
        debug(&f)
    );
}

#[test]
fn a_one_sided_cross_reference_is_not_enough_to_clear_a_shadowed_pair() {
    let f = findings(&[
        ("aaa", "name: aaa\ndescription: Use when reviewing a diff before shipping, checking correctness, finding bugs, judging whether the change is safe to merge."),
        ("bbb", "name: bbb\ndescription: Use when reviewing a diff before merging, checking correctness, finding bugs, judging whether the change is safe to ship. Distinct from aaa."),
    ]);
    assert!(
        f.iter().any(|x| x.0 == "shadowing"),
        "only one side references the other, so the pair is still confusable: {}",
        debug(&f)
    );
}

#[test]
fn a_missing_description_is_p1_nothing_to_route_on() {
    let dir = library(&[("ghost", "name: ghost")]);
    let report = check_skills(&path_str(&dir));
    let miss = report
        .findings
        .iter()
        .find(|f| f.check == "no-description")
        .expect("a skill with no description must be flagged");
    assert_eq!(miss.severity, "P1");
}

#[test]
fn an_over_long_description_is_flagged_as_truncated_by_the_listing() {
    let long = format!(
        "Use when {}",
        "handling an unusual and very specific situation ".repeat(40)
    );
    assert!(long.chars().count() > 1536);
    let f = findings(&[("verbose", &format!("name: verbose\ndescription: {long}"))]);
    assert!(
        f.iter().any(|x| x.0 == "description-truncated"),
        "got {}",
        debug(&f)
    );
}

#[test]
fn folded_yaml_descriptions_are_parsed_not_skipped() {
    // The real skills use folded blocks; if the parser missed them every description would read
    // as empty and the whole check would silently pass.
    let dir = library(&[(
        "folded",
        "name: folded\ndescription: >\n  Use when the description spans several lines\n  and is written as a folded block.",
    )]);
    let skills = load_skills(&path_str(&dir));
    assert_eq!(skills.len(), 1);
    assert!(skills[0].description.contains("folded block"));
}

#[test]
fn a_skill_marked_disable_model_invocation_is_reported_as_not_model_invocable() {
    let dir = library(&[(
        "manual",
        "name: manual\ndescription: Reference material only.\ndisable-model-invocation: true",
    )]);
    let skills = load_skills(&path_str(&dir));
    assert!(!skills[0].model_invocable);
}

#[test]
fn the_real_viby_toolkit_library_is_clean() {
    // Dogfooding: this is the check that would have caught migrate/refactor drifting together.
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../skills")
        .to_string_lossy()
        .into_owned();
    let report = check_skills(&dir);
    assert!(
        report.skills.len() >= 15,
        "expected the real library, found {} skills",
        report.skills.len()
    );
    // This used to exclude `listing-over-budget` by name and then assert it WAS reported, on the
    // belief that 31 skills could never fit. 2026-08-27 disproved that — rewriting every description
    // as pure triggers landed the library at 92% — so the assertion is inverted: no findings at all,
    // and being over budget is now a regression rather than the standing state.
    let found: Vec<String> = report
        .findings
        .iter()
        .map(|f| format!("{}:{}", f.check, f.skills.join("+")))
        .collect();
    assert!(
        found.is_empty(),
        "the shipped library must have no shadowing, no trigger collisions, and must fit the listing budget: {found:?}"
    );
    let total: usize = report
        .skills
        .iter()
        .map(|s| s.description.encode_utf16().count())
        .sum();
    assert!(
        total as u64 <= listing_budget_chars(200_000),
        "descriptions total {total} chars, over the listing budget — the least-used skills get truncated to name-only first, which is exactly how a skill stops firing"
    );
}

#[test]
fn the_listing_budget_matches_the_constants_in_the_claude_code_binary_not_the_docs() {
    // Verified against Claude Code 2.1.220: skillListingBudgetFraction defaults to 0.01, bytesPerToken
    // is 4, and the default context window is 200_000. The binary's own string: "The skill listing is
    // budgeted at ~1% of the context window; when summed descriptions exceed it, entries get truncated
    // and skill routing degrades."
    assert_eq!(listing_budget_chars(200_000), 8_000);
    assert_eq!(listing_budget_chars(1_000_000), 40_000);
}

#[test]
fn not_vacuous_the_aggregate_budget_check_still_fires_when_the_library_overflows() {
    // The real-library test no longer asserts this finding exists, because the library now fits. That
    // removed the only thing keeping the aggregate check honest, so it is pinned on a fixture instead.
    let many: Vec<Skill> = (0..40)
        .map(|i| skill(&format!("s{i}"), &"z".repeat(300)))
        .collect();
    let f = check_listing_budget(&many, DEFAULT_CONTEXT_TOKENS);
    assert!(
        f.iter().any(|x| x.check == "listing-over-budget"),
        "40 x 300 chars is 12,000 against an 8,000 budget"
    );
}

#[test]
fn and_does_not_fire_on_a_library_that_fits() {
    let few: Vec<Skill> = (0..10)
        .map(|i| skill(&format!("s{i}"), &"z".repeat(200)))
        .collect();
    assert!(!check_listing_budget(&few, DEFAULT_CONTEXT_TOKENS)
        .iter()
        .any(|x| x.check == "listing-over-budget"));
}

#[test]
fn a_description_past_the_1536_char_per_skill_cap_is_a_p1_because_the_tail_is_silently_cut() {
    let long = skill("x", &"y".repeat(1_600));
    let f = check_listing_budget(&[long], DEFAULT_CONTEXT_TOKENS);
    assert!(
        f.iter()
            .any(|x| x.check == "description-over-cap" && x.severity == "P1"),
        "over-cap must be P1"
    );
}

#[test]
fn a_skill_name_containing_a_regex_metacharacter_still_gets_the_cross_reference_exemption() {
    // Regression: the name was spliced unescaped into a RegExp, so the pattern for "a+b" meant
    // "one or more a, then b" and never matched the literal name. The mutual-cross-reference
    // exemption silently failed and the pair was reported as a P1 shadowing collision.
    let f = findings(&[
        ("a+b", "name: a+b\ndescription: Use when reviewing a diff before shipping, checking correctness, finding bugs and judging whether the change is safe to merge. Distinct from other."),
        ("other", "name: other\ndescription: Use when reviewing a diff before merging, checking correctness, finding bugs and judging whether the change is safe to ship. Distinct from a+b."),
    ]);
    assert!(
        !f.iter()
            .any(|x| x.0 == "shadowing" || x.0 == "shadowing-watch"),
        "a mutually-disambiguated pair must be exempt regardless of metacharacters, got {}",
        debug(&f)
    );
}

#[test]
fn progressive_disclosure_a_500_plus_line_body_is_p2_and_references_excuses_a_long_but_split_one() {
    // Anthropic's stated limit, fetched 2026-07-29: "Keep the SKILL.md body under 500 lines."
    let base = skill("x", "d");
    let huge = Skill {
        body_lines: 620,
        body_words: 5_000,
        has_references: true,
        ..base.clone()
    };
    assert!(
        check_progressive_disclosure(&[huge])
            .iter()
            .any(|f| f.check == "body-over-500-lines"),
        "past 500 lines is a finding even when references/ exists — the BODY is what loads"
    );
    let split = Skill {
        body_lines: 200,
        body_words: 2_500,
        has_references: true,
        ..base.clone()
    };
    assert!(
        check_progressive_disclosure(&[split]).is_empty(),
        "a long body that HAS been split is fine"
    );
    let unsplit = Skill {
        body_lines: 200,
        body_words: 2_500,
        has_references: false,
        ..base
    };
    assert!(
        check_progressive_disclosure(&[unsplit])
            .iter()
            .any(|f| f.check == "no-progressive-disclosure"),
        "long and unsplit is worth a nudge"
    );
}

#[test]
fn a_damaged_frontmatter_terminator_yields_an_empty_description_which_is_the_p1() {
    // Found by accident while building the A/B harness: a bad edit ate the closing `---`, and the skill
    // then did not appear in `claude -p`'s available-skills list AT ALL — silently, no error. The
    // consequence is worse than "hard to route": the skill does not exist as far as the model is
    // concerned. This gate is what stands between that and shipping.
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("broken")).unwrap();
    fs::write(
        dir.path().join("broken").join("SKILL.md"),
        [
            "---",
            "name: broken",
            "description: >",
            "  a description with no closing fence",
            "",
            "# Body starts here",
        ]
        .join("\n"),
    )
    .unwrap();
    let report = check_skills(&path_str(&dir));
    assert!(
        report
            .findings
            .iter()
            .any(|f| f.check == "no-description" && f.severity == "P1"),
        "must be a P1"
    );
}

#[test]
fn the_pushy_imperative_opener_does_not_count_toward_similarity() {
    // Anthropic's documented fix for undertriggering is a pushy description, and this library
    // measured it working. Once every skill opens "Always load ...", those words are common to all
    // of them: if they counted, adopting the recommended phrasing library-wide would manufacture
    // shadowing findings between skills that share nothing but scaffolding.
    let f = findings(&[
        ("alpha", "name: alpha\ndescription: Always load before writing a database migration — \"add a column\", \"backfill\", \"add an index\"."),
        ("beta", "name: beta\ndescription: Always load before clicking through a rendered page — \"does this render\", \"take a screenshot\", \"the page is blank\"."),
    ]);
    let shadow: Vec<_> = f
        .iter()
        .filter(|x| x.0 == "shadowing" || x.0 == "shadowing-watch")
        .collect();
    assert!(
        shadow.is_empty(),
        "scaffolding counted as overlap: {}",
        debug(&f)
    );
}

#[test]
fn still_not_vacuous_pushy_openers_do_not_hide_a_genuinely_shadowed_pair() {
    let f = findings(&[
        ("alpha", "name: alpha\ndescription: Always load when reviewing a diff before shipping, checking correctness, finding bugs, judging whether the change is safe to merge."),
        ("beta", "name: beta\ndescription: Always load when reviewing a diff before merging, checking correctness, finding bugs, judging whether the change is safe to ship."),
    ]);
    assert_eq!(
        f.iter().filter(|x| x.0 == "shadowing").count(),
        1,
        "expected shadowing, got {}",
        debug(&f)
    );
}
