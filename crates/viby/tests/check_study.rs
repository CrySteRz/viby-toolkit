//! Contract tests for the study auditor.
//!
//! Both halves pinned. This checker audits prose, which is the most false-positive-prone thing in
//! the repo, so the must-NOT half is deliberately larger than the must half: a hedge in a
//! limitations section is correct writing, a number inside a code block is example text, and a
//! figure whose citation sits in the next table row is sourced.

use std::process::Command;

use viby::checks::check_study::{check_study, infer_mode, Mode};

/// A document with every document-level requirement satisfied, so per-line checks can be
/// tested in isolation without the boilerplate findings firing.
fn well_formed_header() -> String {
    [
        "# Study",
        "",
        "> Status: measured 2026-07-29 · 5 angles searched · 22 sources read, 9 cited · 2 claims unverified · 1 prior refuted.",
        "",
        "Stopping rule: saturation after two rounds. Every claim below is labelled measured or inferred.",
        "What would change my mind: an independent replication reporting the opposite direction.",
        "The evidence refuted my starting assumption that the cheap path was equivalent.",
        "",
        "Sources: [a](https://example.com/a) fetched 2026-07-29, [b](https://other.org/b) fetched 2026-07-29.",
        "",
    ]
    .join("\n")
}

fn checks(body: &str) -> Vec<&'static str> {
    check_study(&(well_formed_header() + body), Mode::Study)
        .iter()
        .map(|f| f.check)
        .collect()
}

fn checks_of(text: &str, mode: Mode) -> Vec<&'static str> {
    check_study(text, mode).iter().map(|f| f.check).collect()
}

fn run(args: &[&str]) -> (Option<i32>, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_viby"))
        .arg("check-study")
        .args(args)
        .output()
        .expect("spawn viby");
    (
        out.status.code(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
    )
}

#[test]
fn a_well_formed_study_produces_no_findings() {
    let f = check_study(&well_formed_header(), Mode::Study);
    assert!(f.is_empty(), "expected clean, got {f:#?}");
}

#[test]
fn a_bare_number_with_no_source_is_flagged() {
    let f = checks("\n## Results\n\nThroughput improved by 47% after the change.\n");
    assert!(f.contains(&"unsourced-figure"), "{f:?}");
}

#[test]
fn a_number_whose_citation_is_on_the_same_line_is_not_flagged() {
    let f = checks("\n## Results\n\nThroughput improved by 47% ([bench](https://example.com/x), 2026-07-29).\n");
    assert!(!f.contains(&"unsourced-figure"), "{f:?}");
}

#[test]
fn a_citation_later_in_the_same_wrapped_paragraph_covers_a_figure_earlier_in_it() {
    // Prose wraps. A source on the third line of a bullet still sources that bullet, and firing
    // there would make the check unusable on ordinary writing.
    let f = checks(
        "\n## Results\n\n- The independent run reported 14% precision where the vendor claimed\n  98%, which means the two are not measuring\n  the same thing ([write-up](https://example.com/x), 2026-07-29).\n",
    );
    assert!(!f.contains(&"unsourced-figure"), "{f:?}");
}

#[test]
fn in_a_table_each_row_carries_its_own_source_a_sourced_sibling_row_does_not_cover_it() {
    // Deliberately strict, and it earns its keep: this is the rule that caught a genuinely
    // unsourced row in this repo's own evidence table, which whole-table attribution would have
    // let through. The evidence table is the artifact a reader checks row by row.
    let f = checks(
        "\n## Results\n\n| claim | figure | source |\n|---|---|---|\n| link validity | 94% | [a](https://example.com/a) 2026-07-29 |\n| hallucination rate | 11–57% | reported second-hand |\n",
    );
    assert!(
        f.contains(&"unsourced-figure"),
        "the unsourced row must be caught: {f:?}"
    );
}

#[test]
fn a_number_marked_as_our_own_measurement_is_not_flagged() {
    let f =
        checks("\n## Results\n\nMeasured here on our repo: 19× fewer tokens than the baseline.\n");
    assert!(!f.contains(&"unsourced-figure"), "{f:?}");
}

#[test]
fn a_number_inside_a_fenced_code_block_is_example_text_not_a_claim() {
    let f = checks("\n## Results\n\n```\nlatency: 12.5 ms\nsaving: 47%\n```\n");
    assert!(!f.contains(&"unsourced-figure"), "{f:?}");
}

#[test]
fn small_counts_are_not_treated_as_claims() {
    // "3 angles", "2 candidates" — flagging these would make the checker unusable.
    let f = checks("\n## Results\n\nWe compared 3 candidates across 2 workloads and kept 1.\n");
    assert!(!f.contains(&"unsourced-figure"), "{f:?}");
}

#[test]
fn a_list_whose_introducing_sentence_carries_the_source_is_not_flagged() {
    // Regression: blank lines were treated as starting a block, so the paragraph after a blank
    // line was never recognised as a block start and the introducing sentence was never found.
    // Every properly-introduced list of figures reported one finding per bullet.
    let f = checks(
        "\n## Results\n\nAll three from [the report](https://example.com/report) (fetched 2026-07-29):\n\n- 38% of pages from one year were gone.\n- 25% of pages over the decade were gone.\n- 54% of entries had a dead link.\n",
    );
    assert!(
        !f.contains(&"unsourced-figure"),
        "the intro sentence sources the list: {f:?}"
    );
}

#[test]
fn a_list_with_no_introducing_source_is_still_flagged() {
    let f = checks(
        "\n## Results\n\nHere is what we found:\n\n- 38% of pages from one year were gone.\n",
    );
    assert!(
        f.contains(&"unsourced-figure"),
        "the must-flag half of the same rule"
    );
}

#[test]
fn the_paragraph_directly_above_counts_as_the_citation_for_the_one_below() {
    // Technical writing cites once and then discusses. Requiring the URL in the same paragraph as
    // every restatement fires on correctly sourced prose.
    let f = checks(
        "\n## Results\n\nThe monitored run is reported in [the paper](https://example.com/p) (fetched 2026-07-29),\nwhich gives the figures below.\n\nThe hacked rate fell from 28.57% to 0.56% under monitoring.\n",
    );
    assert!(
        !f.contains(&"unsourced-figure"),
        "an adjacent citing paragraph sources it: {f:?}"
    );
}

#[test]
fn a_paragraph_two_paragraphs_after_the_citation_is_still_flagged() {
    // The bound is what stops one URL from sourcing an entire section.
    let f = checks(
        "\n## Results\n\nSee [the paper](https://example.com/p) (2026-07-29).\n\nSome unrelated discussion with no numbers in it at all.\n\nThe hacked rate fell from 28.57% to 0.56%.\n",
    );
    assert!(
        f.contains(&"unsourced-figure"),
        "the bound must hold: {f:?}"
    );
}

#[test]
fn a_heading_resets_attribution_a_citation_does_not_leak_across_sections() {
    let f = checks(
        "\n## Sources\n\nSee [the paper](https://example.com/p) (2026-07-29).\n\n## Results\n\nThroughput improved 47%.\n",
    );
    assert!(
        f.contains(&"unsourced-figure"),
        "attribution must not cross a heading: {f:?}"
    );
}

#[test]
fn hedging_inside_a_section_headed_not_verified_is_correct_writing_not_a_finding() {
    // Regression: MEASURED_HEADING matched the word "verified" inside "Not verified", so the most
    // honest paragraph in a document was flagged as a result dressed up in hedges.
    for heading in [
        "## Not verified",
        "## Limitations",
        "## Open questions",
        "## Unverified claims",
    ] {
        let f = checks(&format!(
            "\n{heading}\n\nThe paper reportedly builds a dataset; the page is paywalled and was never read.\n"
        ));
        assert!(
            !f.contains(&"hedged-as-measured"),
            "{heading} must allow hedging, got {f:?}"
        );
    }
}

#[test]
fn a_hedge_inside_a_results_section_is_flagged() {
    let f = checks(
        "\n## Measured results\n\nThe index appears to resolve cross-module edges correctly.\n",
    );
    assert!(f.contains(&"hedged-as-measured"), "{f:?}");
}

#[test]
fn the_same_hedge_in_a_limitations_section_is_not_flagged() {
    // Hedging about what you did not verify is honest writing. Flagging it would punish the
    // behaviour this whole skill is trying to encourage.
    let f = checks("\n## Limitations and open questions\n\nThe index appears to resolve cross-module edges, but we did not test it.\n");
    assert!(!f.contains(&"hedged-as-measured"), "{f:?}");
}

#[test]
fn the_title_suggests_in_a_findings_section_is_flagged_the_exact_prose_a_failed_fetch_produces() {
    let f = checks("\n## Findings\n\nWe could not open the PDF; the title suggests it supports our approach.\n");
    assert!(f.contains(&"hedged-as-measured"), "{f:?}");
}

#[test]
fn a_study_with_no_falsifier_is_p1() {
    let text = [
        "# Study",
        "Status: 2026-07-29, 4 angles, saturation reached. Nothing refuted my prior, stated explicitly.",
        "Claim is measured. Source: [a](https://example.com) 2026-07-29",
    ]
    .join("\n");
    let f = check_study(&text, Mode::Study);
    let hit = f
        .iter()
        .find(|x| x.check == "no-falsifier")
        .unwrap_or_else(|| {
            panic!(
                "expected no-falsifier, got {:?}",
                f.iter().map(|x| x.check).collect::<Vec<_>>()
            )
        });
    assert_eq!(hit.severity, "P1");
}

#[test]
fn a_study_citing_exactly_one_domain_is_flagged_as_untriangulated() {
    let text =
        well_formed_header().replace("[b](https://other.org/b)", "[b](https://example.com/b)");
    let f = checks_of(&text, Mode::Study);
    assert!(f.contains(&"single-source"), "{f:?}");
}

#[test]
fn two_independent_domains_are_not_flagged() {
    assert!(!check_study(&well_formed_header(), Mode::Study)
        .iter()
        .any(|x| x.check == "single-source"));
}

#[test]
fn subdomains_of_the_same_host_still_count_as_one_domain() {
    let text = well_formed_header().replace("https://other.org/b", "https://www.example.com/b");
    assert!(
        check_study(&text, Mode::Study)
            .iter()
            .any(|x| x.check == "single-source"),
        "www. must normalise to the same domain"
    );
}

#[test]
fn a_document_with_no_sources_at_all_is_p1() {
    let f = check_study(
        "# Study\n\nStatus: thought about it. What would change my mind: nothing. Refuted: no.\n",
        Mode::Study,
    );
    let hit = f
        .iter()
        .find(|x| x.check == "no-sources")
        .expect("no-sources");
    assert_eq!(hit.severity, "P1");
}

#[test]
fn an_undated_citation_is_reported_at_low_severity() {
    let text =
        well_formed_header() + "\n## Extra\n\nSee [c](https://third.net/c) for background.\n";
    let f = check_study(&text, Mode::Study);
    let hit = f
        .iter()
        .find(|x| x.check == "undated-citation")
        .expect("an undated citation must be reported");
    assert_eq!(
        hit.severity, "P3",
        "undated is a hygiene issue, not a blocker"
    );
}

#[test]
fn missing_stopping_rule_status_line_refutation_and_labels_are_each_caught() {
    let bare = "# Study\n\nThe answer is yes. What would change my mind: a contrary replication.\n\n[a](https://example.com) 2026-07-29\n";
    let f = checks_of(bare, Mode::Study);
    for expected in [
        "no-status-line",
        "no-stopping-rule",
        "nothing-refuted",
        "no-evidence-labels",
    ] {
        assert!(f.contains(&expected), "expected {expected}, got {f:?}");
    }
}

#[test]
fn cli_a_clean_document_exits_0_a_defective_one_exits_1_no_document_exits_2() {
    let dir = tempfile::tempdir().expect("tempdir");
    let good = dir.path().join("good.md");
    let bad = dir.path().join("bad.md");
    std::fs::write(&good, well_formed_header()).expect("write");
    std::fs::write(&bad, "# Study\n\nIt is 47% faster.\n").expect("write");
    let good = good.to_str().expect("utf8 path");
    let bad = bad.to_str().expect("utf8 path");
    assert_eq!(
        run(&[good, "--quiet"]).0,
        Some(0),
        "a well-formed study must pass"
    );
    assert_eq!(
        run(&[bad, "--quiet"]).0,
        Some(1),
        "a defective study must fail"
    );
    assert_eq!(
        run(&["--quiet"]).0,
        Some(2),
        "no document is exit 2, not a pass"
    );
}

#[test]
fn cli_the_report_states_it_checks_form_not_truth() {
    // A clean run must never read as "this study is verified" — the checks it cannot do are the
    // ones that matter most.
    let dir = tempfile::tempdir().expect("tempdir");
    let good = dir.path().join("good.md");
    std::fs::write(&good, well_formed_header()).expect("write");
    let (_, stdout) = run(&[good.to_str().expect("utf8 path")]);
    assert!(stdout.contains("checks FORM, not truth"), "{stdout}");
    assert!(stdout.contains("whether a quote is real"), "{stdout}");
}

#[test]
fn mode_is_decided_from_path_and_title_not_from_the_documents_own_fields() {
    // The critical property: if the mode were inferred from the presence of a status line, a study
    // that forgot its status line would silently opt out of every other structural check — one
    // missing field disarming the gates meant to catch it.
    let naked = "# Study of caching approaches\n\nno status line here at all\n";
    assert_eq!(
        infer_mode("scratch.md", naked),
        Mode::Study,
        "an H1 saying Study means study mode"
    );
    assert_eq!(
        infer_mode("docs/studies/x.md", "# Untitled\n"),
        Mode::Study,
        "the studies/ path means study mode"
    );
    assert_eq!(
        infer_mode("2026-caching-research.md", "# Untitled\n"),
        Mode::Study,
        "a research filename means study mode"
    );
    assert_eq!(
        infer_mode("references/methods.md", "# Where the rules come from\n"),
        Mode::Notes
    );
}

#[test]
fn a_study_missing_its_status_line_is_still_held_to_the_rest_of_the_contract() {
    let naked =
        "# Study of caching\n\nThe answer is yes. See https://example.com and https://other.org\n";
    let f = checks_of(naked, infer_mode("study.md", naked));
    assert!(f.contains(&"no-status-line"), "{f:?}");
    assert!(
        f.contains(&"no-falsifier"),
        "the other checks must still run: {f:?}"
    );
}

#[test]
fn notes_mode_keeps_the_per_line_checks_but_drops_the_study_structure_ones() {
    let notes = "# Where the rules come from\n\n## Results\n\nIt is 47% faster.\n";
    let f = checks_of(notes, Mode::Notes);
    assert!(
        f.contains(&"unsourced-figure"),
        "sourcing still matters in notes: {f:?}"
    );
    for structural in [
        "no-status-line",
        "no-falsifier",
        "nothing-refuted",
        "no-stopping-rule",
        "no-sources",
    ] {
        assert!(
            !f.contains(&structural),
            "{structural} must not fire in notes mode, got {f:?}"
        );
    }
}

#[test]
fn a_locally_measured_study_with_no_external_sources_is_p3_not_p1() {
    // A labelled local measurement is the strongest evidence for your own context; demanding
    // citations for it would push the author toward decorative references.
    let local = [
        "# Study: which extraction approach",
        "Status: measured 2026-07-29 on our own repo against a hand-established oracle. 1 prior refuted.",
        "Stopping rule: exhaustion — three candidates is the whole population.",
        "Ground truth: four fixtures, now contract tests. Labelled measured or inferred throughout.",
        "What would change my mind: a fixture where the chosen approach loses.",
    ]
    .join("\n");
    let f = check_study(&local, Mode::Study);
    let hit = f
        .iter()
        .find(|x| x.check == "no-sources")
        .expect("it should still be mentioned");
    assert_eq!(
        hit.severity, "P3",
        "a declared local measurement is not a sourcing defect"
    );
    assert!(hit.message.contains("locally-measured"));
}

#[test]
fn a_study_with_no_sources_and_no_local_evidence_marker_stays_p1() {
    let vague = [
        "# Study: which approach",
        "Status: I thought about it for a while. Nothing refuted.",
        "Stopping rule: saturation. Claims are inferred.",
        "What would change my mind: better arguments.",
    ]
    .join("\n");
    let f = check_study(&vague, Mode::Study);
    let hit = f.iter().find(|x| x.check == "no-sources");
    assert_eq!(
        hit.map(|x| x.severity),
        Some("P1"),
        "unsourced opinion is still unsourced"
    );
}

#[test]
fn an_unreadable_file_is_reported_never_silently_treated_as_clean() {
    let (status, stdout) = run(&["/nope/missing.md", "--json"]);
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap_or_default();
    let found = parsed["findings"]
        .as_array()
        .is_some_and(|a| a.iter().any(|f| f["check"] == "unreadable"));
    assert!(found, "{stdout}");
    assert_eq!(status, Some(1));
}

#[test]
fn a_figure_in_a_section_reporting_the_documents_own_benchmark_needs_no_external_citation() {
    // Largest false-positive class, found by running this on two real human-written research docs:
    // 28 of 34 unsourced-figure findings were in a section headed "Benchmarked on our own repo",
    // where every number WAS the author's measurement. Demanding a citation for your own result is
    // wrong, and it trains the reader to ignore the check.
    let f = checks("\n## Benchmarked on our own repo\n\nThe index resolved 0 of the 2 cross-app importers, and the query cost 380 tokens.\n");
    assert!(
        !f.contains(&"unsourced-figure"),
        "self-measured sections are self-sourced: {f:?}"
    );
}

#[test]
fn a_figure_in_an_ordinary_section_still_needs_a_citation() {
    let f = checks("\n## Background\n\nThe best model reaches 41.58% on this task.\n");
    assert!(
        f.contains(&"unsourced-figure"),
        "the must-flag half of the same rule"
    );
}

#[test]
fn a_semver_is_not_an_unsourced_figure() {
    // Regression: the `s` unit had no trailing boundary, so "viby-toolkit 2.21.3 so its skills"
    // matched as "21.3 s" and every version string in a document was reported as a measurement.
    let pad = "\nfiller.\n".repeat(6);
    let found = checks(&format!(
        "{pad}- **Decision served:** what to change in viby-toolkit 2.21.3 so its skills fan out.\n{pad}"
    ));
    assert!(
        !found.contains(&"unsourced-figure"),
        "semver flagged: {}",
        found.join(", ")
    );
}

#[test]
fn a_real_duration_measurement_is_still_an_unsourced_figure() {
    // Padded away from the fixture's Sources line: a nearby URL legitimately suppresses this check,
    // so without the padding the test would pass for the wrong reason.
    let pad = "\nfiller.\n".repeat(6);
    let found = checks(&format!(
        "{pad}The p99 latency was 12.4 s under load.\n{pad}"
    ));
    assert!(
        found.contains(&"unsourced-figure"),
        "the boundary fix must not blind the checker to real units"
    );
}

#[test]
fn a_decimal_measurement_with_a_plural_unit_and_no_space_is_still_an_unsourced_figure() {
    // Regression: the trailing \b added to fix the semver false positive rejected "1.5MBs", silently
    // letting a real unsourced measurement through. Both directions are pinned.
    let pad = "\nfiller.\n".repeat(6);
    let found = checks(&format!("{pad}We cut memory to 1.5MBs per request.\n{pad}"));
    assert!(
        found.contains(&"unsourced-figure"),
        "plural unit missed: {}",
        found.join(", ")
    );
}
