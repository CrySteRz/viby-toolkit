//! viby-toolkit UserPromptSubmit skill hint (OPT-IN: needs `AI_GATEWAY_API_KEY` and `VIBY_DECIDE=on`).
//!
//! Asks a decision model which skill fits the prompt and, only when it is confident, adds one line
//! of context naming it. Advisory: it never invokes a skill. Silent on any failure, and always exit 0.
//! When on, the prompt text is sent to Vercel AI Gateway; that is why it is opt-in.
//!
//! Measured on `tests/routing/probes*.tsv`: 97–100 % top-1 for the default models
//! (`docs/studies/2026-10-23-decision-model-mammoth.md`).

use std::io::{Read, Write};

use serde_json::{json, Map, Value};

use super::decide;
use crate::checks::check_skills::load_skills;

/// Below this the hint is more likely noise than help.
const MIN_CONFIDENCE: f64 = 0.9;
const MAX_DESCRIPTION: usize = 600;
const MAX_PROMPT: usize = 4000;
const MIN_WORDS: usize = 4;

fn clip(s: &str, max: usize) -> &str {
    match s.char_indices().nth(max) {
        Some((i, _)) => &s[..i],
        None => s,
    }
}

/// `{skill: description}` for every skill the model may invoke, plus `none`.
fn criteria(skills_dir: &str) -> Option<Value> {
    let mut map = Map::new();
    for s in load_skills(skills_dir)
        .into_iter()
        .filter(|s| s.model_invocable)
    {
        map.insert(
            s.name,
            Value::String(clip(&s.description, MAX_DESCRIPTION).to_string()),
        );
    }
    if map.is_empty() {
        return None;
    }
    map.insert(
        "none".into(),
        "No skill fits: casual chat, a factual question, or a tiny one-line edit that needs no workflow.".into(),
    );
    Some(Value::Object(map))
}

/// Whether a hint could change anything, decided before any network call. Not for a slash command
/// or a prompt that names a skill (the person already chose), nor for a reply of under four words
/// ("yes", "go on"), which continues the current task rather than starting one. The shortest
/// labelled routing probe is five words.
fn worth_asking(prompt: &str) -> bool {
    !prompt.starts_with('/')
        && !prompt.to_ascii_lowercase().contains("viby-toolkit:")
        && prompt.split_whitespace().count() >= MIN_WORDS
}

/// The hint line, or None when there is nothing worth saying.
fn hint(input: &str, skills_dir: &str) -> Option<String> {
    let event: Value = serde_json::from_str(input).ok()?;
    let prompt = event.get("prompt").and_then(Value::as_str)?.trim();
    if !worth_asking(prompt) {
        return None;
    }
    let criteria = criteria(skills_dir)?;
    let questions = json!({"skill": {
        "type": "choice",
        "instructions": "Which workflow skill should handle this request? Pick none if no skill fits.",
        "criteria": criteria,
    }});
    let state = format!(
        "A developer typed this request to their coding agent: {}",
        Value::String(clip(prompt, MAX_PROMPT).to_string())
    );
    let decision = decide::evaluate(&state, &questions)?;
    let answer = decision.answers.get("skill")?;
    let choice = answer.get("choice").and_then(Value::as_str)?;
    let confidence = answer
        .get("confidence")
        .and_then(Value::as_f64)
        .or_else(|| answer.get("probabilities")?.get(choice)?.as_f64())?;
    // Only a skill that was offered: the answer is a third party's text going into Claude's context.
    if choice == "none" || confidence < MIN_CONFIDENCE || criteria.get(choice).is_none() {
        return None;
    }
    Some(format!(
        "Skill hint: viby-toolkit:{choice} (confidence {confidence:.2}, from a decision model; use it if it fits)"
    ))
}

/// Args: the skills directory (default `$CLAUDE_PLUGIN_ROOT/skills`).
pub fn main(args: &[String]) -> i32 {
    if !decide::opted_in() {
        return 0;
    }
    let skills_dir = match args.first() {
        Some(dir) => dir.clone(),
        None => match std::env::var("CLAUDE_PLUGIN_ROOT") {
            Ok(root) if !root.is_empty() => format!("{root}/skills"),
            _ => return 0,
        },
    };
    let mut input = Vec::new();
    let _ = std::io::stdin().read_to_end(&mut input);
    if let Some(line) = hint(&String::from_utf8_lossy(&input), &skills_dir) {
        let _ = writeln!(std::io::stdout().lock(), "{line}");
    }
    0
}
