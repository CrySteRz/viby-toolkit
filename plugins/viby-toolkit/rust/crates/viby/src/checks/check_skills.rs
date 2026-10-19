//! viby-toolkit skill-library health check — guards against skill shadowing.
//!
//! "More Skills, Worse Agents?" (arXiv 2605.24050) measured what happens as a skill library
//! grows: pass rate drops up to 21% at 202 skills (~8% at 52, ~14% at 102), and the fraction
//! of runs that invoke the RIGHT skill falls from 88% to 53%. The important part is the cause:
//!
//!   - the dominant mechanism is SKILL SHADOWING — a skill whose description semantically
//!     overlaps another's hides it from selection, exactly like variable shadowing;
//!   - the cost of the extra CONTEXT is "statistically indistinguishable from zero".
//!
//! So the thing to police is not how many skills exist, nor how many tokens they cost. It is
//! how distinguishable their descriptions are. The paper's own recommended mitigation is
//! "description disambiguation" — which is what this check makes checkable.
//!
//! Usage:
//!   viby check-skills [skills-dir] [--json] [--quiet] [--against dir]...
//! Exit: 0 = clean, 1 = findings, 2 = no skills found.
//!
//! Port of `check-skills.ts`. This module also hosts the frontmatter helpers (`frontmatter`,
//! `yaml_field`, `fence_index`) that `check_agents` and `check_hooks` reuse.

use std::collections::HashSet;
use std::fs;
use std::path::Path;

use regex::Regex;
use serde_json::{json, Value};

use crate::core::args::{flag, parse_or_exit, values};
use crate::core::js::{is_js_space, js_trim, read_text, utf16_len, JS_SPACE};
use crate::core::nodepath::{read_dir_sorted, resolve};

#[derive(Clone, Debug)]
pub struct Finding {
    pub check: String,
    pub severity: &'static str,
    pub skills: Vec<String>,
    pub message: String,
    pub detail: Option<String>,
    /// The TypeScript wrote the cross-library findings with `severity` before `check`; the JSON
    /// key order is part of the output, so it is kept.
    severity_first: bool,
}

impl Finding {
    fn new(
        check: &str,
        severity: &'static str,
        skills: Vec<String>,
        message: String,
        detail: Option<&str>,
    ) -> Finding {
        Finding {
            check: check.to_string(),
            severity,
            skills,
            message,
            detail: detail.map(str::to_string),
            severity_first: false,
        }
    }

    fn severity_first(mut self) -> Finding {
        self.severity_first = true;
        self
    }

    pub fn to_json(&self) -> Value {
        let mut map = serde_json::Map::new();
        if self.severity_first {
            map.insert("severity".into(), json!(self.severity));
            map.insert("check".into(), json!(self.check));
        } else {
            map.insert("check".into(), json!(self.check));
            map.insert("severity".into(), json!(self.severity));
        }
        map.insert("skills".into(), json!(self.skills));
        map.insert("message".into(), json!(self.message));
        if let Some(d) = &self.detail {
            map.insert("detail".into(), json!(d));
        }
        Value::Object(map)
    }
}

#[derive(Clone, Debug)]
pub struct Skill {
    pub name: String,
    pub description: String,
    /// Quoted phrases a user might literally type.
    pub triggers: Vec<String>,
    pub model_invocable: bool,
    /// Simultaneous instructions the body asks the model to honour.
    pub directives: usize,
    pub body_lines: usize,
    pub body_words: usize,
    pub has_references: bool,
}

pub struct SkillsReport {
    pub skills: Vec<Skill>,
    pub findings: Vec<Finding>,
}

// ---- frontmatter helpers -----------------------------------------------------------------

/// `text.indexOf("\n---", 3)`: the 3 counts UTF-16 units, so convert it to a byte offset first.
pub fn fence_index(text: &str) -> Option<usize> {
    let mut units = 0;
    let mut start = text.len();
    for (i, c) in text.char_indices() {
        if units >= 3 {
            start = i;
            break;
        }
        units += c.len_utf16();
    }
    text[start..].find("\n---").map(|i| i + start)
}

pub fn frontmatter(text: &str) -> &str {
    if !text.starts_with("---") {
        return "";
    }
    match fence_index(text) {
        Some(end) => &text[3..end],
        None => "",
    }
}

/// Pull a possibly-multiline YAML scalar (`key: >` folded blocks included).
pub fn yaml_field(fm: &str, key: &str) -> String {
    // JavaScript's `^`, `$` and `.` break lines at \r, U+2028 and U+2029 as well as \n, so the
    // line start is `^` or a consumed terminator and the value stops at any of the four.
    let pattern = format!(
        r"(?m)(?:^|[\r  ]){}:[{JS_SPACE}]*(>-?|\|-?)?[ \t]*([^\n\r  ]*)",
        regex::escape(key)
    );
    let Ok(re) = Regex::new(&pattern) else {
        return String::new();
    };
    let Some(caps) = re.captures(fm) else {
        return String::new();
    };
    if caps.get(1).is_some_and(|g| !g.as_str().is_empty()) {
        // folded/literal block: take the indented lines that follow
        let whole = caps.get(0).map_or("", |m| m.as_str());
        let m0 = whole.trim_start_matches(['\r', '\u{2028}', '\u{2029}']);
        let start = fm.find(m0).unwrap_or(0) + m0.len();
        let mut body: Vec<&str> = Vec::new();
        for line in fm[start..].split('\n') {
            if js_trim(line).is_empty() {
                continue;
            }
            if line.chars().take_while(|&c| is_js_space(c)).count() < 2 {
                break;
            }
            body.push(js_trim(line));
        }
        return body.join(" ");
    }
    js_trim(caps.get(2).map_or("", |m| m.as_str())).to_string()
}

// ---- the check ---------------------------------------------------------------------------

/// Instruction-count thresholds, measured rather than guessed.
///
/// "Prompt Design at Scale" (arXiv 2607.19257) found perfect response rates collapse to ZERO
/// by N=80 simultaneous instructions for every model tested, regardless of format — "a hard
/// floor rather than a gradual asymptote" — and recommends treating ~40 as a REDESIGN
/// threshold rather than a tuning point.
///
/// A skill body is exactly a list of simultaneous instructions, so this applies directly. For
/// reference, the largest skill in this repo sits at 32.
const DIRECTIVES_REDESIGN: usize = 40;
const DIRECTIVES_FLOOR: usize = 80;

/// Words that carry no routing signal. Without stripping these, every description looks
/// similar to every other because they share the scaffolding of an English sentence.
const STOPWORDS: &str = concat!(
    "a an the and or but if then than that this these those use used using when whether while for",
    " to of in on at by with from into out up down over under again further once here there all any",
    " both each few more most other some such no nor not only own same so too very can will just",
    " should now also it its is are was were be been being do does did doing have has had you your",
    " i we they he she them their what which who whom how why where says say said want wants asks",
    " ask before after during about against between through above below own s t don now d ll m o re",
    " ve y viby code skill skills claude also e.g eg ie",
    // The pushy-imperative scaffolding itself. Anthropic's guidance is to make descriptions
    // "a little bit pushy" to fight undertriggering, and this library measured that working. Once
    // every description opens "Always load ...", those words appear in all 31 and inflate every
    // pairwise similarity — the metric would punish the recommended phrasing and reward vagueness,
    // exactly what the mutual-cross-reference exemption below already refuses to do.
    " always load loading loads"
);

fn words(text: &str) -> HashSet<String> {
    let stop: HashSet<&str> = STOPWORDS.split_whitespace().collect();
    // Everything outside [a-z0-9-] separates words, so a word is a run of those characters.
    text.to_lowercase()
        .split(|c: char| !(c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'))
        .filter(|w| w.len() > 2 && !stop.contains(w))
        .map(str::to_string)
        .collect()
}

/// Quoted phrases in a description — the literal utterances that route a request.
///
/// The TypeScript pattern was `["“']([^"”']{4,60})["”']`; the length counts UTF-16 units, so the
/// scan is written out rather than using a character-counting regex.
pub fn trigger_phrases(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let is_close = |c: char| c == '"' || c == '”' || c == '\'';
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if matches!(chars[i], '"' | '“' | '\'') {
            let mut j = i + 1;
            let mut units = 0;
            while j < chars.len() && !is_close(chars[j]) {
                units += chars[j].len_utf16();
                j += 1;
            }
            if j < chars.len() && (4..=60).contains(&units) {
                let raw: String = chars[i + 1..j].iter().collect();
                let phrase = js_trim(&raw).to_lowercase();
                if phrase.chars().any(is_js_space) {
                    out.push(phrase);
                }
                i = j + 1;
                continue;
            }
        }
        i += 1;
    }
    out
}

/// Does `text` mention `name`? The name is escaped first: it comes from frontmatter, so a
/// name containing a regex metacharacter (`a+b`, `c#`, `f.sharp`) was previously spliced raw
/// into a RegExp — `\ba+b\b` means "one or more a, then b" and never matches the literal
/// "a+b". That silently broke the mutual-cross-reference exemption and produced a wrong P1.
fn mentions(text: &str, name: &str) -> bool {
    let pattern = format!(
        r"(^|[^A-Za-z0-9_-]){}([^A-Za-z0-9_-]|$)",
        regex::escape(name)
    );
    Regex::new(&pattern).is_ok_and(|re| re.is_match(text))
}

fn jaccard(a: &HashSet<String>, b: &HashSet<String>) -> f64 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let shared = a.iter().filter(|w| b.contains(*w)).count();
    shared as f64 / (a.len() + b.len() - shared) as f64
}

/// `x.toFixed(0)` for a non-negative `x`: JavaScript rounds an exact tie up, as `round` does
/// (Rust's `{:.0}` rounds ties to even).
fn fixed0(x: f64) -> String {
    format!("{}", x.round() as i64)
}

pub fn load_skills(dir: &str) -> Vec<Skill> {
    let mut out = Vec::new();
    let dir = Path::new(dir);
    let Some(entries) = read_dir_sorted(dir) else {
        return out;
    };
    let line_item = Regex::new(&format!(r"^[{JS_SPACE}]*[-*][{JS_SPACE}]+[^{JS_SPACE}]")).ok();
    let numbered = Regex::new(&format!(
        r"^[{JS_SPACE}]*[0-9]+\.[{JS_SPACE}]+[^{JS_SPACE}]"
    ))
    .ok();
    let disabled = Regex::new(&format!(
        r"(?m)(?:^|[\r\u2028\u2029])disable-model-invocation:[{JS_SPACE}]*true"
    ))
    .ok();
    for entry in entries {
        let Some(text) = read_text(dir.join(&entry).join("SKILL.md")) else {
            continue;
        };
        let fm = frontmatter(&text);
        let description = [yaml_field(fm, "description"), yaml_field(fm, "when_to_use")]
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        // A directive is one instruction the model must honour simultaneously: a bullet or a
        // numbered step. Prose sentences are context; bullets are the instruction budget.
        let body = match fence_index(&text) {
            Some(i) => &text[i + 4..],
            None => &text[..],
        };
        let directives = body
            .split('\n')
            .filter(|l| {
                line_item.as_ref().is_some_and(|r| r.is_match(l))
                    || numbered.as_ref().is_some_and(|r| r.is_match(l))
            })
            .count();
        let body_lines = body.split('\n').count();
        let body_words = body.split(is_js_space).filter(|w| !w.is_empty()).count();
        let has_references = fs::read_dir(dir.join(&entry).join("references"))
            .map(|rd| {
                rd.filter_map(|e| e.ok())
                    .any(|e| e.file_name().to_string_lossy().ends_with(".md"))
            })
            .unwrap_or(false);
        let name = match yaml_field(fm, "name") {
            n if n.is_empty() => entry.clone(),
            n => n,
        };
        out.push(Skill {
            body_lines,
            body_words,
            has_references,
            name,
            triggers: trigger_phrases(&description),
            description,
            model_invocable: !disabled.as_ref().is_some_and(|r| r.is_match(fm)),
            directives,
        });
    }
    out
}

/// CALIBRATED AGAINST REAL DATA, not intuition.
///
/// Measured over a real 19-skill library: median pairwise similarity 2.1%, and the most
/// similar legitimate pair — `migrate` vs `refactor`, which genuinely are adjacent — sits at
/// 13%. Jaccard over short descriptions with stopwords removed simply does not produce large
/// numbers, so the first thresholds tried here (0.38 / 0.50, picked by feel) could never have
/// fired on any realistic library. That is worse than no check: it reports "clean" forever.
///
/// So: watch just below the most-adjacent real pair, and high well above anything a
/// deliberately-distinguished library produces.
const SHADOW_HIGH: f64 = 0.2; // a pair this close is near-duplicate framing; selection is a coin flip
const SHADOW_WATCH: f64 = 0.12; // adjacent enough that one disambiguating clause each is worth it

/// Shadowing across LIBRARIES, not just within one.
///
/// This checker only ever compared a directory against itself, which means it never saw the pairs that
/// matter most: dispatch competes across every installed plugin. Measured 2026-07-29 — a fresh agent
/// offered 90 skills picked a different plugin's `security-review` as runner-up for a probe this
/// library owns. Cross-library collisions cannot be fixed by editing the other plugin, so they are
/// reported at P2 with the only available remedy: make OUR description more specific.
pub fn check_across(our_dir: &str, other_dirs: &[String]) -> Vec<Finding> {
    let ours: Vec<Skill> = load_skills(our_dir)
        .into_iter()
        .filter(|s| !s.description.is_empty())
        .collect();
    let mut findings = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for dir in other_dirs {
        let theirs: Vec<Skill> = load_skills(dir)
            .into_iter()
            .filter(|s| !s.description.is_empty())
            .collect();
        for a in &ours {
            for b in &theirs {
                if a.name == b.name {
                    continue;
                }
                let key = format!("{}|{}", a.name, b.name);
                if seen.contains(&key) {
                    continue;
                }
                let sim = jaccard(&words(&a.description), &words(&b.description));
                if sim >= SHADOW_HIGH {
                    seen.insert(key);
                    findings.push(
                        Finding::new(
                            "cross-library-shadowing",
                            "P2",
                            vec![a.name.clone(), format!("{} (external)", b.name)],
                            format!("{}% description overlap with an installed skill from another plugin — dispatch chooses between them and you can only edit yours", fixed0(sim * 100.0)),
                            None,
                        )
                        .severity_first(),
                    );
                }
                for phrase in trigger_phrases(&a.description) {
                    if b.description.to_lowercase().contains(&phrase) {
                        let k2 = format!("t:{}|{}|{}", a.name, b.name, phrase);
                        if seen.contains(&k2) {
                            continue;
                        }
                        seen.insert(k2);
                        findings.push(
                            Finding::new(
                                "cross-library-trigger",
                                "P1",
                                vec![a.name.clone(), format!("{} (external)", b.name)],
                                format!("both claim the literal trigger \"{phrase}\" — one of them wins and it is not decided by you"),
                                None,
                            )
                            .severity_first(),
                        );
                    }
                }
            }
        }
    }
    findings
}

/// The skill listing is budgeted, and overflowing it silently degrades routing. Verified against the
/// Claude Code binary (2.1.220), not the docs: `skillListingBudgetFraction` defaults to `0.01`,
/// `bytesPerToken` is `4`, the default context window is `200_000`, and `skillListingMaxDescChars`
/// caps a single description at `1536`. The binary's own words:
///
///   "The skill listing is budgeted at ~1% of the context window; when summed descriptions exceed
///    it, entries get truncated and skill routing degrades."
///
/// And the overflow order is the part that hurts: descriptions are dropped starting with the skills
/// you invoke LEAST. A fresh session has no invocation history, so a library's descriptions are the
/// first to be cut to name-only — which is why writing LONGER, more explicit descriptions to fix a
/// mis-route makes it worse. Every added character raises the overflow that strips the keywords.
const LISTING_BYTES_PER_TOKEN: f64 = 4.0;
const LISTING_BUDGET_FRACTION: f64 = 0.01;
const LISTING_MAX_DESC_CHARS: usize = 1536;
/// The context window the TypeScript assumed when none was given.
pub const DEFAULT_CONTEXT_TOKENS: u64 = 200_000;
/// The gate is PER-SKILL, not on the total, because a per-skill number is the actionable one.
///
/// It used to say a 31-skill library "cannot fit" an 8,000-char budget however it is written. That
/// was wrong, and 2026-08-27 disproved it: rewriting all 31 as pure triggers — pushy opener, quoted
/// utterances, no summary of what the skill does — took the library 10,321 -> 7,370 chars, 129% ->
/// 92%, with zero shadowing findings. 258 chars each is tight, not impossible. What is
/// actually controllable is the length of each description, and the total follows from it.
const DESC_TARGET: usize = 400;

pub fn listing_budget_chars(context_tokens: u64) -> u64 {
    (LISTING_BUDGET_FRACTION * context_tokens as f64 * LISTING_BYTES_PER_TOKEN).floor() as u64
}

/// JavaScript's number-to-string for the `contextTokens / 1000` in the message.
fn js_number(x: f64) -> String {
    format!("{x}")
}

pub fn check_listing_budget(skills: &[Skill], context_tokens: u64) -> Vec<Finding> {
    let mut findings = Vec::new();
    let total: usize = skills.iter().map(|s| utf16_len(&s.description)).sum();
    let budget = listing_budget_chars(context_tokens);
    let share = total as f64 / budget as f64;

    for s in skills {
        let len = utf16_len(&s.description);
        if len > LISTING_MAX_DESC_CHARS {
            findings.push(Finding::new(
                "description-over-cap",
                "P1",
                vec![s.name.clone()],
                format!("{len} chars — past the {LISTING_MAX_DESC_CHARS}-char per-skill cap, so the tail is cut off and whatever routing keywords live there are simply gone"),
                Some("the cut is silent; nothing warns you that the phrase you added is not in the listing"),
            ));
        } else if len > DESC_TARGET {
            findings.push(Finding::new(
                "description-too-long",
                "P2",
                vec![s.name.clone()],
                format!("{len} chars against a {DESC_TARGET}-char target — it is spending listing budget that gets taken from the whole library"),
                Some("cut the summary of what the skill DOES and keep only the triggers; the body says what it does"),
            ));
        }
    }
    if share > 1.0 {
        findings.push(Finding::new(
            "listing-over-budget",
            // P3 deliberately: it is a library-wide symptom whose fix is spread across every description,
            // so the ACTIONABLE half is the per-skill P2. It IS clearable — the library sits at 92% — so
            // treat it as a real regression signal rather than as standing background noise.
            "P3",
            vec![],
            format!(
                "{} descriptions total {} chars = {}% of the {}-char listing budget at {}k context, so entries get truncated and routing decides on less than you wrote",
                skills.len(),
                total,
                fixed0(share * 100.0),
                budget,
                js_number(context_tokens as f64 / 1000.0)
            ),
            Some(
                "descriptions are dropped least-invoked-first, so in a fresh session these are the first cut \
                 to name-only. Informational: with this many skills it is a standing cost, not a bug to fix.",
            ),
        ));
    }
    findings
}

/// Progressive disclosure. Anthropic's own guidance, fetched 2026-07-29: "Keep the SKILL.md body under
/// 500 lines for optimal performance. If your content exceeds this, split it into separate files using
/// the progressive disclosure patterns" — and, on nesting: "Claude may partially read files when
/// they're referenced from other referenced files… Keep references one level deep from SKILL.md."
///
/// The word threshold is the softer signal and it is P3 on purpose: a long body is only a problem if
/// none of it has been moved out, and what counts as "too long" depends on how often the skill loads.
const BODY_LINES_MAX: usize = 500;
const BODY_WORDS_WATCH: usize = 1_800;

pub fn check_progressive_disclosure(skills: &[Skill]) -> Vec<Finding> {
    let mut findings = Vec::new();
    for s in skills {
        if s.body_lines > BODY_LINES_MAX {
            findings.push(Finding::new(
                "body-over-500-lines",
                "P2",
                vec![s.name.clone()],
                format!("{} lines — past Anthropic's stated {BODY_LINES_MAX}-line limit for a SKILL.md body", s.body_lines),
                Some("move the sections only needed at point of use into references/, one level deep"),
            ));
        } else if s.body_words > BODY_WORDS_WATCH && !s.has_references {
            findings.push(Finding::new(
                "no-progressive-disclosure",
                "P3",
                vec![s.name.clone()],
                format!("{} words in one body with no references/ — every word loads whenever the skill fires", s.body_words),
                Some("split the parts a reader only needs once they are acting on that section"),
            ));
        }
    }
    findings
}

pub fn check_skills(dir: &str) -> SkillsReport {
    let skills = load_skills(dir);
    let mut findings: Vec<Finding> = check_listing_budget(&skills, DEFAULT_CONTEXT_TOKENS);
    findings.extend(check_progressive_disclosure(&skills));

    for s in &skills {
        if js_trim(&s.description).is_empty() {
            findings.push(Finding::new(
                "no-description",
                "P1",
                vec![s.name.clone()],
                "no description parsed — the skill is DROPPED FROM THE LISTING ENTIRELY, silently. \
                 Observed 2026-07-30: a SKILL.md whose frontmatter terminator was damaged did not appear in \
                 `claude -p`'s available skills at all, with no error and no warning. Usually a malformed \
                 frontmatter block rather than a genuinely absent description — check the `---` fences first"
                    .to_string(),
                None,
            ));
        }
        if s.directives >= DIRECTIVES_FLOOR {
            findings.push(Finding::new(
                "instruction-overload",
                "P1",
                vec![s.name.clone()],
                format!("{} directives — at N=80 simultaneous instructions, perfect compliance measured zero for every model tested, regardless of format", s.directives),
                Some("split it into a focused skill plus an on-demand reference file, as review does"),
            ));
        } else if s.directives >= DIRECTIVES_REDESIGN {
            findings.push(Finding::new(
                "instruction-heavy",
                "P2",
                vec![s.name.clone()],
                format!("{} directives — past ~40, treat this as a redesign threshold rather than something to tune", s.directives),
                Some("move the reference material into references/ and keep the skill body to the decisions"),
            ));
        }
        // Claude Code truncates description + when_to_use at 1,536 chars in the listing.
        let len = utf16_len(&s.description);
        if len > 1536 {
            findings.push(Finding::new(
                "description-truncated",
                "P2",
                vec![s.name.clone()],
                format!("description is {len} chars and is truncated at 1536 in the skill listing — the tail is invisible to routing"),
                None,
            ));
        }
    }

    // Pairwise shadowing. Overlap, not count, is what the research found degrades selection.
    let invocable: Vec<&Skill> = skills
        .iter()
        .filter(|s| s.model_invocable && !js_trim(&s.description).is_empty())
        .collect();
    for i in 0..invocable.len() {
        for j in i + 1..invocable.len() {
            let (a, b) = (invocable[i], invocable[j]);
            let sim = jaccard(&words(&a.description), &words(&b.description));

            // An explicit cross-reference is the disambiguation, so it must not read as a problem.
            // A description saying "distinct from X — use X for Y" REDUCES the chance the model
            // confuses the pair while INCREASING their word overlap. Scoring that as shadowing
            // would penalise the fix and reward vagueness, so a mutually-referencing pair is
            // treated as already disambiguated.
            let a_names_b = mentions(&a.description, &b.name);
            let b_names_a = mentions(&b.description, &a.name);
            if a_names_b && b_names_a {
                continue;
            }

            let pair = vec![a.name.clone(), b.name.clone()];
            if sim >= SHADOW_HIGH {
                findings.push(Finding::new(
                    "shadowing",
                    "P1",
                    pair.clone(),
                    format!("descriptions are {}% similar — one will shadow the other and the model will pick between them arbitrarily", fixed0(sim * 100.0)),
                    Some("add an explicit boundary to each: say what it is NOT for, and which sibling to use instead"),
                ));
            } else if sim >= SHADOW_WATCH {
                findings.push(Finding::new(
                    "shadowing-watch",
                    "P3",
                    pair.clone(),
                    format!("descriptions are {}% similar — still distinguishable, but one disambiguating clause each would keep it that way", fixed0(sim * 100.0)),
                    None,
                ));
            }

            // A literal trigger phrase claimed by two skills is the sharpest possible collision:
            // the user types exactly that, and routing is a coin flip.
            let shared: Vec<&String> = a
                .triggers
                .iter()
                .filter(|t| b.triggers.contains(t))
                .collect();
            if !shared.is_empty() {
                findings.push(Finding::new(
                    "duplicate-trigger",
                    "P1",
                    pair,
                    format!(
                        "both claim the same trigger phrase(s): {}",
                        shared
                            .iter()
                            .map(|t| format!("\"{t}\""))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                    Some("give the phrase to exactly one skill, and have the other name it as an explicit exclusion"),
                ));
            }
        }
    }

    SkillsReport { skills, findings }
}

fn severity_rank(s: &str) -> u8 {
    match s {
        "P1" => 0,
        "P2" => 1,
        _ => 2,
    }
}

pub fn main(args: &[String]) -> i32 {
    let p = match parse_or_exit(args, &[flag("json"), flag("quiet"), values("against")]) {
        Ok(p) => p,
        Err(code) => return code,
    };
    let as_json = p.flag("json");
    let quiet = p.flag("quiet");
    let dir = resolve(&[p
        .positionals
        .first()
        .map_or("plugins/viby-toolkit/skills", String::as_str)]);
    let SkillsReport {
        skills,
        mut findings,
    } = check_skills(&dir);
    // Dispatch competes across every installed library, not just within this one.
    let others: Vec<String> = p
        .values("against")
        .iter()
        .filter(|d| **d != dir && Path::new(d).exists())
        .cloned()
        .collect();
    if !others.is_empty() {
        findings.extend(check_across(&dir, &others));
    }

    if skills.is_empty() {
        if as_json {
            println!("{{\"skills\":0,\"findings\":[]}}");
        } else if !quiet {
            println!("no skills found under {dir}");
        }
        return 2;
    }

    let exit = if findings.is_empty() { 0 } else { 1 };
    if as_json {
        let doc = json!({
            "skills": skills.len(),
            "findings": findings.iter().map(Finding::to_json).collect::<Vec<_>>(),
        });
        println!("{}", serde_json::to_string_pretty(&doc).unwrap_or_default());
        return exit;
    }

    findings.sort_by_key(|f| severity_rank(f.severity));
    for f in &findings {
        println!("[{} {}] {}", f.severity, f.check, f.skills.join(" ↔ "));
        println!("    {}", f.message);
        if let Some(d) = &f.detail {
            println!("    fix: {d}");
        }
    }

    if !quiet {
        let blocked: Vec<&str> = skills
            .iter()
            .filter(|s| !s.model_invocable)
            .map(|s| s.name.as_str())
            .collect();
        println!();
        println!(
            "{} skills ({})",
            skills.len(),
            if blocked.is_empty() {
                "all model-invocable".to_string()
            } else {
                format!("{} not model-invocable", blocked.join(", "))
            }
        );
        // Honest framing: report where this library sits on the measured curve rather than
        // implying any count is inherently unsafe.
        let note = if skills.len() < 52 {
            "below the smallest library size where degradation was measured (52 skills, ~8% drop)"
        } else if skills.len() < 102 {
            "in the range where a ~8-14% selection drop was measured"
        } else {
            "in the range where a 14-21% selection drop was measured — consider retrieval-based pre-filtering"
        };
        println!("library size is {note}.");
        // `sort` is stable, so the first of equals wins.
        let mut heaviest: Option<&Skill> = None;
        for s in &skills {
            if heaviest.is_none_or(|h| s.directives > h.directives) {
                heaviest = Some(s);
            }
        }
        if let Some(h) = heaviest {
            println!(
                "heaviest skill: {} at {} directives (redesign at {DIRECTIVES_REDESIGN}, compliance floor at {DIRECTIVES_FLOOR}).",
                h.name, h.directives
            );
        }
        println!(
            "Overlap matters more than count: the measured degradation is driven by shadowing, while\n\
             the cost of the extra context was indistinguishable from zero (arXiv 2605.24050)."
        );
        if findings.is_empty() {
            println!("\n✓ no shadowing or trigger collisions found");
        }
    }
    exit
}
