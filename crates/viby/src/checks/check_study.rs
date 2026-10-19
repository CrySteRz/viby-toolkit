//! viby-toolkit study auditor — the executable half of /viby-toolkit:study.
//!
//! Checks the structural properties of a research document that are mechanically decidable, and
//! refuses to guess at the ones that aren't. It exists because the alternative — asking a model
//! to score its own report on a quality rubric — is measurably unreliable: LLM-defined rubrics
//! misalign with expert judgement, are coarse, and push the judge onto knowledge nobody can
//! verify (see references/methods.md). So this checks form, a human checks substance.
//!
//! Usage:
//!   viby check-study <file.md> [more.md...] [--json] [--quiet]
//! Exit: 0 = clean, 1 = findings, 2 = nothing to check.
//!
//! WHAT IT CANNOT DO, stated so a clean run is not mistaken for a verified study: it does no
//! network access, so it cannot tell you whether a URL resolves, whether a quoted sentence really
//! appears at that URL, or whether the source says what you claim. Those are the checks that
//! matter most and they are yours. A clean exit means "the document has the parts a checkable
//! study needs", never "the study is right".

use std::collections::HashSet;
use std::path::MAIN_SEPARATOR;
use std::sync::LazyLock;

use regex::Regex;
use serde_json::{json, Map, Value};

use crate::core::args::{flag, parse_or_exit};

// JavaScript's `\s`: Unicode white space without U+0085, plus the byte-order mark. The `regex`
// crate's own `\s` differs on both, and so does its `\b`, `\w` and `\d`, so every pattern below
// spells them out or opts out of Unicode with `(?-u:..)` / `(?i-u:..)`.
const JS_S: &str = r"\t\n\x0B\x0C\r \u{a0}\u{1680}\u{2000}-\u{200a}\u{2028}\u{2029}\u{202f}\u{205f}\u{3000}\u{feff}";

fn is_js_space(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n' | '\u{b}' | '\u{c}' | '\r' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200a}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202f}'
                | '\u{205f}'
                | '\u{3000}'
                | '\u{feff}'
    )
}

/// JavaScript's `String.prototype.trim`.
fn trim(s: &str) -> &str {
    s.trim_matches(is_js_space)
}

fn rx(pattern: &str) -> Regex {
    Regex::new(pattern).expect("static regex")
}

#[derive(Debug, Clone)]
pub struct Finding {
    pub line: usize,
    pub check: &'static str,
    pub severity: &'static str,
    pub message: String,
    pub fix: &'static str,
}

/// Hedges that mean "I did not verify this". Harmless in a limitations section; a defect inside
/// a claim presented as measured.
const HEDGES: &[&str] = &[
    "appears to",
    "appear to",
    "seems to",
    "seem to",
    "presumably",
    "the title suggests",
    "suggests that",
    "likely that",
    "it is likely",
    "probably",
    "i believe",
    "should be roughly",
    "reportedly",
];

/// Section headings whose content is being presented as a measurement.
///
/// NEGATED first: a section headed "Not verified", "Limitations" or "Open questions" is where
/// hedging BELONGS. Matching it flagged the most honest paragraph in a document as if it were a
/// result — punishing exactly the behaviour the skill asks for.
static NOT_MEASURED_HEADING: LazyLock<Regex> = LazyLock::new(|| {
    rx(
        r"(?i-u:\b(not |un)(verified|tested|measured|proven)|limitation|open question|caveat|unknown|future work)",
    )
});
static MEASURED_HEADING: LazyLock<Regex> = LazyLock::new(|| {
    rx(r"(?i-u:\b(measur|benchmark|result|finding|verified|tested|experiment|data))")
});

/// A section reporting the document's OWN measurement. Figures there need no external citation —
/// the document IS the source. Demanding one is wrong, and it is the largest false-positive class
/// this checker had: run against two real human-written research documents it produced 34
/// unsourced-figure findings, 28 of them in a section headed "Benchmarked on our own repo" whose
/// every number was the author's own measured result.
static SELF_MEASURED: LazyLock<Regex> = LazyLock::new(|| {
    rx(
        r"(?i-u:\bbenchmark(ed)?\b|\bwe (?:ran|measured|tested|built|stood up)\b|\bour own (?:repo|code|data|machine)\b|\bmeasured (?:here|locally|on|against)\b|\btest run\b|\bresults? \d|\bon (?:this|our) repo\b)",
    )
});

fn presented_as_measured(heading: &str) -> bool {
    MEASURED_HEADING.is_match(heading) && !NOT_MEASURED_HEADING.is_match(heading)
}

/// A quantity strong enough that citing it unsourced is a defect. Bare small integers are not
/// claims ("2 candidates", "3 angles"), so they do not count.
///
/// The trailing \b on the unit group is load-bearing: without it the `s` unit matched the first
/// letter of the next word, so "viby-toolkit 2.21.3 so its skills" parsed as "21.3 s" and every
/// semver in a document was reported as an unsourced measurement.
///
/// The `s?` before it is equally load-bearing in the other direction: the boundary alone rejected a
/// plural unit written without a space ("cut memory to 1.5MBs"), silently letting a real unsourced
/// measurement through. Both halves are pinned by tests.
static FIGURE: LazyLock<Regex> = LazyLock::new(|| {
    rx(&format!(
        r"[0-9][0-9,]*\.?[0-9]*[{JS_S}]?%|[0-9][0-9,]*\.?[0-9]*[{JS_S}]?[×x](?-u:\b)|(?-u:\b)n[{JS_S}]?=[{JS_S}]?[0-9]+|\$[{JS_S}]?[0-9][0-9,]*|[0-9][0-9,]{{4,}}(?-u:\b)|[0-9]+\.[0-9]+[{JS_S}]?(ms|s|GB|MB|kB|k tokens|tokens)s?(?-u:\b)"
    ))
});

static URL_RE: LazyLock<Regex> = LazyLock::new(|| rx(&format!(r#"https?://[^{JS_S})\]<>"']+"#)));
static HAS_URL: LazyLock<Regex> = LazyLock::new(|| rx("https?://"));
static DATE_RE: LazyLock<Regex> = LazyLock::new(|| {
    rx(&format!(
        r"(?-u:\b)(20[0-9]{{2}}-[0-9]{{2}}-[0-9]{{2}}|(?-u:\b)(?:Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Oct|Nov|Dec)[a-z]*[{JS_S}]+20[0-9]{{2}}|(?-u:\b)20[0-9]{{2}}(?-u:\b))"
    ))
});

static ISO_DATE: LazyLock<Regex> =
    LazyLock::new(|| rx(r"(?-u:\b)20[0-9]{2}-[0-9]{2}-[0-9]{2}(?-u:\b)"));
static YEAR: LazyLock<Regex> = LazyLock::new(|| rx(r"(?-u:\b)(19|20)[0-9]{2}(?-u:\b)"));
static CLOCK: LazyLock<Regex> = LazyLock::new(|| rx(r"(?-u:\b)[0-9]{1,2}:[0-9]{2}(?-u:\b)"));

/// Dates are not quantities. A year or an ISO date is four digits and would otherwise trip the
/// bare-integer branch of FIGURE on every properly date-stamped citation — punishing the exact
/// habit this checker is meant to encourage.
fn blank_dates(line: &str) -> String {
    let a = ISO_DATE.replace_all(line, "");
    let b = YEAR.replace_all(&a, "");
    CLOCK.replace_all(&b, "").into_owned()
}

static URL_HOST: LazyLock<Regex> = LazyLock::new(|| rx("^https?://([^/]+)"));

fn domain_of(url: &str) -> String {
    let host = URL_HOST
        .captures(url)
        .and_then(|m| m.get(1))
        .map_or("", |m| m.as_str());
    host.strip_prefix("www.").unwrap_or(host).to_lowercase()
}

static HEADING_START: LazyLock<Regex> =
    LazyLock::new(|| rx(&format!(r"^[{JS_S}]*#{{1,6}}[{JS_S}]")));
static HEADING_START_PLUS: LazyLock<Regex> =
    LazyLock::new(|| rx(&format!(r"^[{JS_S}]*#{{1,6}}[{JS_S}]+")));
static LIST_ITEM: LazyLock<Regex> =
    LazyLock::new(|| rx(&format!(r"^[{JS_S}]*([-*+]|[0-9]+\.)[{JS_S}]")));
static TABLE_ROW: LazyLock<Regex> = LazyLock::new(|| rx(&format!(r"^[{JS_S}]*\|")));
static QUOTE: LazyLock<Regex> = LazyLock::new(|| rx(&format!(r"^[{JS_S}]*>")));
static FENCE: LazyLock<Regex> = LazyLock::new(|| rx(&format!(r"^[{JS_S}]*```")));

/// Group lines into blocks — a paragraph, one list item (including its wrapped continuation
/// lines), or one table row. A citation anywhere in the block covers the whole block, which is
/// how a reader attributes it. Line windows do not work here: a three-line bullet whose source
/// sits on its third line reported two unsourced figures, and a checker that fires on correctly
/// sourced prose gets switched off.
pub fn block_of(lines: &[&str]) -> Vec<usize> {
    let mut owner = Vec::with_capacity(lines.len());
    let mut start = 0;
    for (i, line) in lines.iter().enumerate() {
        // A blank line ENDS the current block; the next line opens a fresh one. Treating it as the
        // start of a block made every paragraph after a blank line inherit the blank line's index,
        // so no paragraph was ever its own block start and the "sentence introducing this list"
        // lookup silently never found anything.
        if trim(line).is_empty() {
            owner.push(i);
            start = i + 1;
            continue;
        }
        // A multi-line blockquote is ONE unit — a quotation spanning three lines is not three
        // claims, and splitting it meant the second line of a quote could not see the citation that
        // introduced it. Table rows stay per-row on purpose (§ evidence tables are read row by row).
        let is_quote = QUOTE.is_match(line);
        let prev_is_quote = i > 0 && QUOTE.is_match(lines[i - 1]);
        if HEADING_START.is_match(line)
            || LIST_ITEM.is_match(line)
            || TABLE_ROW.is_match(line)
            || (is_quote && !prev_is_quote)
        {
            start = i;
        }
        owner.push(start);
    }
    owner
}

/// Fenced code blocks are not prose: a figure or hedge inside one is example text, not a claim.
fn mask_code_blocks(lines: &[&str]) -> Vec<bool> {
    let mut in_code = Vec::with_capacity(lines.len());
    let mut open = false;
    for line in lines {
        if FENCE.is_match(line) {
            open = !open;
            in_code.push(true);
            continue;
        }
        in_code.push(open);
    }
    in_code
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Study,
    Notes,
}

impl Mode {
    pub fn as_str(self) -> &'static str {
        match self {
            Mode::Study => "study",
            Mode::Notes => "notes",
        }
    }
}

static NOTES_PATH: LazyLock<Regex> =
    LazyLock::new(|| rx(r"(?i-u:(^|/)(references|reference|notes|appendix)/)"));
static STUDY_PATH: LazyLock<Regex> =
    LazyLock::new(|| rx(r"(?i-u:(^|/)docs/(studies|research|decisions|adr)/)"));
static STUDY_NAME: LazyLock<Regex> = LazyLock::new(|| rx(r"(?i-u:stud(y|ies)|research|spike)"));
static STUDY_H1: LazyLock<Regex> = LazyLock::new(|| {
    rx(&format!(
        r"^[{JS_S}]*(?i-u:(a |the )?(study|research|spike|evaluation|investigation)\b)"
    ))
});

fn is_line_terminator(c: char) -> bool {
    matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}')
}

/// `/^#\s+(.*)$/m.exec(text)?.[1]`: the first heading-1 line. `\s+` is greedy and also eats line
/// breaks, so "#\n\nTitle" yields "Title", exactly as the pattern does.
fn first_h1(text: &str) -> &str {
    let mut prev: Option<char> = None;
    for (at, c) in text.char_indices() {
        let line_start = prev.is_none_or(is_line_terminator);
        prev = Some(c);
        if !line_start || c != '#' {
            continue;
        }
        let after = &text[at + 1..];
        let rest = after.trim_start_matches(is_js_space);
        if rest.len() == after.len() {
            continue;
        }
        let end = rest.find(is_line_terminator).unwrap_or(rest.len());
        return &rest[..end];
    }
    ""
}

/// Which contract to hold this document to.
///
/// Deliberately decided from the PATH and TITLE, never from the document's content. Inferring it
/// from the presence of a status line would let a study missing its status line silently opt out
/// of every other structural check — one absent field disarming the gates that would have caught
/// it. This repo has shipped that exact class of defect once before.
pub fn infer_mode(file: &str, text: &str) -> Mode {
    let norm = file.replace(MAIN_SEPARATOR, "/");
    // Path wins over title, because a title is prose and prose lies. "Where the study rules come
    // from" is a reference file about studies, not a study — and a decision record is a study
    // under another name. Getting this backwards gives the real study the weaker audit, which is
    // exactly what happened the first time this ran.
    if NOTES_PATH.is_match(&norm) {
        return Mode::Notes;
    }
    if STUDY_PATH.is_match(&norm) {
        return Mode::Study;
    }
    if STUDY_NAME.is_match(norm.rsplit('/').next().unwrap_or("")) {
        return Mode::Study;
    }
    if STUDY_H1.is_match(first_h1(text)) {
        return Mode::Study;
    }
    Mode::Notes
}

/// Evidence that a document's claims rest on a local measurement rather than on citations.
static LOCAL_EVIDENCE: LazyLock<Regex> = LazyLock::new(|| {
    rx(
        r"(?i-u:\b(oracle|ground truth|fixture|measured (here|on|locally)|our own repo|contract test))",
    )
});
static LOCAL_DECLARED: LazyLock<Regex> = LazyLock::new(|| {
    rx(r"(?i-u:local measurement|no external (citation|source)|measured on (this|our) repo)")
});
static CITED: LazyLock<Regex> = LazyLock::new(|| rx(r"\[[^\]]+\]\([^)]+\)|\[\^?[0-9]+\]|§"));
static CITED_WORDS: LazyLock<Regex> = LazyLock::new(|| {
    rx(r"(?i-u:\barXiv\b|\bDOI\b|\bfetched\b|\bmeasured (?:here|locally|on)\b|\bref\b)")
});

static HAS_STATUS: LazyLock<Regex> =
    LazyLock::new(|| rx("status[: ]|what was actually done|searched|sources read"));
static HAS_FALSIFIER: LazyLock<Regex> = LazyLock::new(|| {
    rx("would change (my|the) mind|would change (this|the) (conclusion|answer)|falsif|what would decide")
});
static HAS_REFUTED: LazyLock<Regex> =
    LazyLock::new(|| rx("refut|corrected|proved wrong|contradict|i was wrong|prior was"));
static HAS_STOPPING: LazyLock<Regex> = LazyLock::new(|| {
    rx(
        r"stopping rule|saturation|top [0-9]+|exhaust|effort[^\n\r\u{2028}\u{2029}]bounded|searched until",
    )
});
static HAS_LABELS: LazyLock<Regex> =
    LazyLock::new(|| rx("measured|inferred|not tested|unverified|verified"));

/// `s.slice(0, n)` where `n` counts UTF-16 units. A cut through a surrogate pair leaves a lone
/// high surrogate, which Rust cannot hold: it is kept as a private-use stand-in
/// (U+10F800 + offset) that `render` turns into what JavaScript printed.
fn slice_utf16(s: &str, n: usize) -> String {
    let mut out = String::new();
    let mut units = 0;
    for c in s.chars() {
        let width = c.len_utf16();
        if units + width > n {
            if width == 2 && units + 1 == n {
                let high = ((c as u32 - 0x10000) >> 10) + 0xD800;
                out.push(char::from_u32(0x10F800 + high - 0xD800).unwrap_or('\u{FFFD}'));
            }
            break;
        }
        out.push(c);
        units += width;
    }
    out
}

/// Turn the stand-ins of `slice_utf16` into U+FFFD (plain output, where a lone surrogate is
/// encoded as that) or a `\udXXX` escape (JSON, where `JSON.stringify` escapes it).
fn render(text: &str, as_json: bool) -> String {
    text.chars()
        .map(|c| match c as u32 {
            n @ 0x10F800..=0x10FBFF => {
                if as_json {
                    format!("\\u{:04x}", n - 0x10F800 + 0xD800)
                } else {
                    "\u{FFFD}".to_string()
                }
            }
            _ => c.to_string(),
        })
        .collect()
}

pub fn check_study(text: &str, mode: Mode) -> Vec<Finding> {
    let lines: Vec<&str> = text.split('\n').collect();
    let in_code = mask_code_blocks(&lines);
    let owner = block_of(&lines);
    let mut findings = Vec::new();
    let lower = text.to_lowercase();

    // ---- per-line checks
    let mut current_heading = "";
    // The heading plus the first few lines under it — where a section declares its own benchmark.
    let mut section_head = String::new();
    let mut section_head_len = 0usize;
    // The last ordinary sentence before a list or table — its citation covers the block it
    // introduces. Without this, "All three from [source]:" followed by three bullets reports
    // three unsourced figures, which trains the reader to ignore the check.
    let mut intro_line = String::new();
    // Last line index of the intro paragraph. An intro only covers the block IMMEDIATELY below
    // it — without this bound, one URL early in the document silently sourced every list and
    // table after it, which is source laundering with extra steps.
    let mut intro_end: i64 = -99;
    // First line of the current run of list items / table rows. The intro covers the WHOLE run,
    // not just its first item — otherwise the third bullet of a properly-introduced list is
    // "unsourced" purely because it is further from the sentence.
    let mut run_start: i64 = -1;
    // The paragraph before the current one, and where it ended. Technical writing cites a source
    // once and then discusses it, so the paragraph directly above counts as the citation for the
    // one below — bounded to *directly* above, which is what stops a single URL from sourcing a
    // whole section.
    let mut prev_para = String::new();
    let mut prev_para_end: i64 = -99;
    let mut domains: HashSet<String> = HashSet::new();
    let mut url_count = 0usize;
    let mut undated_citations = 0usize;
    let mut first_undated = 1usize;

    for (i, line) in lines.iter().copied().enumerate() {
        if in_code[i] {
            continue;
        }
        if HEADING_START_PLUS.is_match(line) {
            current_heading = line;
            section_head = line.to_string();
            section_head_len = line.encode_utf16().count();
            intro_line.clear();
            intro_end = -99;
            prev_para.clear();
            prev_para_end = -99;
            continue;
        }

        if section_head_len < 600 {
            section_head.push('\n');
            section_head.push_str(line);
            section_head_len += 1 + line.encode_utf16().count();
        }
        let urls: Vec<&str> = URL_RE.find_iter(line).map(|m| m.as_str()).collect();
        for u in &urls {
            url_count += 1;
            domains.insert(domain_of(u));
        }

        // A figure with no source on the same line and none on the next (tables and footnotes
        // routinely put the citation in the adjacent cell or line).
        let is_block_item = LIST_ITEM.is_match(line) || TABLE_ROW.is_match(line);
        if !is_block_item && !trim(line).is_empty() {
            // Accumulate the WHOLE paragraph, not just its first line: a citation frequently lands on
            // the second or third line of the sentence that introduces a figure, and keeping only the
            // first line reported those as unsourced.
            if owner[i] == i {
                prev_para = std::mem::take(&mut intro_line);
                prev_para_end = intro_end;
                intro_line = line.to_string();
            } else {
                intro_line = format!("{intro_line}\n{line}");
            }
            intro_end = i as i64;
            run_start = -1; // ordinary prose ends any list/table run
        }
        // The figure's own block (paragraph / list item / table row), plus the sentence introducing
        // the block — but only when that sentence is directly above it.
        let block_start = owner[i];
        let mut block_end = block_start;
        while block_end + 1 < lines.len() && owner[block_end + 1] == block_start {
            block_end += 1;
        }
        if is_block_item && run_start == -1 {
            run_start = block_start as i64;
        }
        let intro = if is_block_item {
            if run_start >= 0 && run_start - intro_end <= 2 {
                intro_line.as_str()
            } else {
                ""
            }
        } else if block_start as i64 - prev_para_end <= 2 {
            prev_para.as_str()
        } else {
            ""
        };
        let block_text = lines[block_start..=block_end].join("\n");
        let near = format!("{block_text}\n{intro}");

        // A cited source with no date anywhere in its block. Sources rot — a quarter of pages that
        // existed over a recent decade are already gone — so an undated citation may be unverifiable
        // later, and nobody will know when it was true. Checked per BLOCK, not per line: a wrapped
        // citation routinely carries "fetched <date>" on its second line.
        if !urls.is_empty() && !DATE_RE.is_match(&block_text) {
            undated_citations += 1;
            if undated_citations == 1 {
                first_undated = i + 1;
            }
        }

        let self_measured = SELF_MEASURED.is_match(current_heading)
            || SELF_MEASURED.is_match(&near)
            || SELF_MEASURED.is_match(&section_head);
        if FIGURE.is_match(&blank_dates(line))
            && !self_measured
            && !HAS_URL.is_match(&near)
            && !CITED.is_match(&near)
            && !CITED_WORDS.is_match(&near)
        {
            findings.push(Finding {
                line: i + 1,
                check: "unsourced-figure",
                severity: "P1",
                message: format!(
                    "a quantity with no source on or next to its line: \"{}\"",
                    slice_utf16(trim(line), 90)
                ),
                fix: "attach the source and the quoted sentence, or mark it as your own measurement — an unattributed number is the most repeated citation defect there is",
            });
        }

        // A hedge inside a section that presents itself as measured. The exact prose this repo
        // has produced before when a fetch silently failed: "appears to", "the title suggests".
        if presented_as_measured(current_heading) {
            let lowered = line.to_lowercase();
            if let Some(hit) = HEDGES.iter().find(|h| lowered.contains(**h)) {
                findings.push(Finding {
                    line: i + 1,
                    check: "hedged-as-measured",
                    severity: "P1",
                    message: format!(
                        "\"{hit}\" inside a section headed {} — hedged language presented as a result",
                        slice_utf16(trim(current_heading), 50)
                    ),
                    fix: "either verify it and state it plainly, or move it out of the results section and label it inferred / not tested",
                });
            }
        }
    }

    // ---- document-level checks
    if url_count > 0 && domains.len() == 1 {
        let only = domains.iter().next().map_or("", String::as_str);
        findings.push(Finding {
            line: 1,
            check: "single-source",
            severity: "P1",
            message: format!("every citation points at one domain ({only}) — no triangulation, and if that source is wrong the study is wrong"),
            fix: "find an independent source, or state plainly that the conclusion rests on a single source",
        });
    }
    if undated_citations > 0 {
        findings.push(Finding {
            line: first_undated,
            check: "undated-citation",
            severity: "P3",
            message: format!("{undated_citations} citation line(s) carry no date — sources rot, and an undated link cannot be re-checked against what it said"),
            fix: "record the date you fetched it alongside the URL",
        });
    }

    // Everything below is the contract for a STUDY. A notes/reference file legitimately has no
    // status line or falsifier, and demanding them there is scope noise, not a finding.
    if mode == Mode::Notes {
        return findings;
    }

    if !HAS_STATUS.is_match(&lower) {
        findings.push(Finding {
            line: 1,
            check: "no-status-line",
            severity: "P2",
            message: "no status line saying what was actually done (date, angles searched, sources read vs cited, claims unverified)".into(),
            fix: "add it as the first thing, so a reader can weigh the document before reading it",
        });
    }
    if !HAS_FALSIFIER.is_match(&lower) {
        findings.push(Finding {
            line: 1,
            check: "no-falsifier",
            severity: "P1",
            message: "no \"what would change this conclusion\" — an unfalsifiable study cannot be checked or revisited".into(),
            fix: "state the observations that would overturn the answer, before you become attached to it",
        });
    }
    if !HAS_REFUTED.is_match(&lower) {
        findings.push(Finding {
            line: 1,
            check: "nothing-refuted",
            severity: "P2",
            message: "nothing recorded as refuted or corrected — a study that changed none of its author's beliefs usually tested none of them".into(),
            fix: "record what the evidence contradicted, including your own starting assumption; if genuinely nothing, say so explicitly",
        });
    }
    if !HAS_STOPPING.is_match(&lower) {
        findings.push(Finding {
            line: 1,
            check: "no-stopping-rule",
            severity: "P2",
            message: "no stated stopping rule, so the reader cannot tell thorough from tired"
                .into(),
            fix: "name it: saturation, effort-bounded (and the N), or exhaustion",
        });
    }
    if !HAS_LABELS.is_match(&lower) {
        findings.push(Finding {
            line: 1,
            check: "no-evidence-labels",
            severity: "P2",
            message: "no measured / inferred / not-tested labelling anywhere — unlabelled inferences get promoted to results by the summary".into(),
            fix: "label each claim at the point it appears, not in a closing caveat",
        });
    }
    if url_count == 0 {
        // A study whose evidence is a local measurement legitimately cites nothing external — that
        // is the strongest evidence there is for your own context. It just has to say so.
        let local = LOCAL_EVIDENCE.is_match(text);
        // Declared outright — nothing left to tell the author.
        if LOCAL_DECLARED.is_match(text) {
            return findings;
        }
        findings.push(Finding {
            line: 1,
            check: "no-sources",
            severity: if local { "P3" } else { "P1" },
            message: if local {
                "no external sources — fine for a locally-measured study, but the reader has to be told that is what this is"
            } else {
                "no sources cited at all"
            }
            .into(),
            fix: if local {
                "say in the status line that the evidence is a local measurement, and name the oracle"
            } else {
                "if this is genuinely a first-principles argument, say so in the status line — otherwise cite what you read"
            },
        });
    }

    findings
}

static STUDY_FILE: LazyLock<Regex> = LazyLock::new(|| rx(r"(?i-u:\.(md|markdown|txt)$)"));

fn order(severity: &str) -> u8 {
    match severity {
        "P1" => 0,
        "P2" => 1,
        _ => 2,
    }
}

struct Reported {
    file: String,
    finding: Finding,
    /// The unreadable finding lists `file` first; every other one lists it last.
    file_first: bool,
}

impl Reported {
    fn to_json(&self) -> Value {
        let f = &self.finding;
        if self.file_first {
            json!({"file": self.file, "line": f.line, "check": f.check, "severity": f.severity,
                "message": f.message, "fix": f.fix})
        } else {
            json!({"line": f.line, "check": f.check, "severity": f.severity,
                "message": f.message, "fix": f.fix, "file": self.file})
        }
    }
}

pub fn main(args: &[String]) -> i32 {
    let parsed = match parse_or_exit(
        args,
        &[flag("json"), flag("quiet"), flag("study"), flag("notes")],
    ) {
        Ok(p) => p,
        Err(code) => return code,
    };
    let (json_out, quiet) = (parsed.flag("json"), parsed.flag("quiet"));

    let files: Vec<&String> = parsed
        .positionals
        .iter()
        .filter(|p| STUDY_FILE.is_match(p))
        .collect();
    if files.is_empty() {
        if json_out {
            println!("{}", json!({"files": 0, "findings": []}));
        } else if !quiet {
            println!("no study document given (expects a .md path)");
        }
        return 2;
    }

    let mut all: Vec<Reported> = Vec::new();
    let mut modes: Vec<(String, Mode)> = Vec::new();
    let mut read = 0usize;
    for f in files {
        let text = match std::fs::read(f) {
            Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
            Err(_) => {
                all.push(Reported {
                    file: f.clone(),
                    finding: Finding {
                        line: 1,
                        check: "unreadable",
                        severity: "P2",
                        message: "could not read this file, so it was NOT checked".into(),
                        fix: "fix the path and re-run — do not treat this run as clean for it",
                    },
                    file_first: true,
                });
                continue;
            }
        };
        read += 1;
        let mode = if parsed.flag("study") {
            Mode::Study
        } else if parsed.flag("notes") {
            Mode::Notes
        } else {
            infer_mode(f, &text)
        };
        // A repeated path keeps its first position and takes the latest mode, as a Map does.
        match modes.iter_mut().find(|(name, _)| name == f) {
            Some(entry) => entry.1 = mode,
            None => modes.push((f.clone(), mode)),
        }
        for finding in check_study(&text, mode) {
            all.push(Reported {
                file: f.clone(),
                finding,
                file_first: false,
            });
        }
    }

    let code = i32::from(!all.is_empty());
    if json_out {
        let mut mode_map = Map::new();
        for (file, mode) in &modes {
            mode_map.insert(file.clone(), Value::String(mode.as_str().to_string()));
        }
        let findings: Vec<Value> = all.iter().map(Reported::to_json).collect();
        let out = json!({"files": read, "modes": mode_map, "findings": findings});
        let text = serde_json::to_string_pretty(&out).unwrap_or_default();
        println!("{}", render(&text, true));
        return code;
    }

    all.sort_by_key(|r| (order(r.finding.severity), r.finding.line));
    for r in &all {
        let f = &r.finding;
        println!("{}:{}  [{} {}]", r.file, f.line, f.severity, f.check);
        println!("    {}", render(&f.message, false));
        println!("    fix: {}", f.fix);
    }

    if !quiet {
        println!();
        for (file, mode) in &modes {
            // Always say which contract was applied: a clean run in notes mode must never be mistaken
            // for passing the full study contract.
            match mode {
                Mode::Study => println!("{file}: audited as a STUDY (full contract)"),
                Mode::Notes => println!(
                    "{file}: audited as NOTES — structural study checks skipped; pass --study to enforce them"
                ),
            }
        }
        if all.is_empty() {
            println!("clean: {read} document(s) have the parts their contract requires");
        } else {
            println!("{} finding(s) across {read} document(s)", all.len());
        }
        println!(
            "This checks FORM, not truth. It does no network access, so it cannot tell you whether a\n\
             link resolves, whether a quote is real, or whether the source supports the claim — and\n\
             that last one fails often enough to be the main job. See /viby-toolkit:study."
        );
    }
    code
}
