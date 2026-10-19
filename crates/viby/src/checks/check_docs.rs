//! viby-toolkit docs auditor — the executable half of /viby-toolkit:docs.
//!
//! `docs` says every command, path and claim must be one you have actually run or verified, because
//! documentation is the easiest place to ship something confidently false — and a wrong command in a
//! README costs more than ten missing pages, since a stale doc is *trusted*.
//!
//! This automates the mechanically checkable part of that: does the path exist, is the script real,
//! does the link resolve, does the anchor exist.
//!
//! Usage:
//!   viby check-docs [paths...] [--root <repo-root>] [--json] [--quiet]
//! Exit: 0 = clean, 1 = findings, 2 = no docs found.
//!
//! WHAT IT CANNOT DO: it does not run anything, so it cannot tell you a documented command still
//! *works* — only that the script it names exists. It cannot check prose claims, external URLs (no
//! network), or whether the example output is still what the tool prints. A clean run means the
//! document's references resolve, not that the document is true.
//!
//! Port notes: the file index keeps its walk order (entries in byte order, depth first, last
//! directory first, as libuv's `scandir` made the TypeScript see them) because the first suffix
//! match decides which file a link's anchor is checked in. Regexes follow JavaScript: ASCII `\w`
//! and `/i`, JS `\s`, `^` after any JS line terminator. The plain report sorts files with an
//! ICU-style `localeCompare` for ASCII names (punctuation, digits, letters, lowercase first) and
//! approximately beyond it (see `weight`).

use crate::core::args::{flag, parse_or_exit, value};
use fancy_regex::Regex as Fancy;
use regex::Regex;
use serde::Serialize;
use serde_json::{json, Value};
use std::cmp::Ordering;
use std::collections::HashSet;
use std::fs;
use std::sync::LazyLock;

#[derive(Clone, Debug, Serialize)]
pub struct Finding {
    pub file: String,
    pub line: usize,
    pub rule: String,
    pub severity: String,
    pub problem: String,
    pub fix: String,
}

/// JS `\s`: not Rust's, which has U+0085 and lacks U+FEFF.
const JS_WS: &str = "\t\n\u{b}\u{c}\r \u{a0}\u{1680}\u{2000}-\u{200a}\u{2028}\u{2029}\u{202f}\u{205f}\u{3000}\u{feff}";
/// JS `^` under `/m`: the start of the text or just after any JS line terminator.
const BOL: &str = "(?:^|(?<=[\n\r\u{2028}\u{2029}]))";
const SKIP: &[&str] = &[
    ".git",
    "node_modules",
    "vendor",
    "dist",
    "build",
    "target",
    ".next",
    "coverage",
    ".venv",
    "venv",
];

fn is_js_ws(c: char) -> bool {
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

macro_rules! rx {
    ($name:ident, $pat:expr) => {
        static $name: LazyLock<Option<Regex>> = LazyLock::new(|| Regex::new(&$pat).ok());
    };
}
macro_rules! fancy {
    ($name:ident, $pat:expr) => {
        static $name: LazyLock<Option<Fancy>> = LazyLock::new(|| Fancy::new(&$pat).ok());
    };
}

// A path reference worth resolving: has a slash and an extension, or is a known dir form.
rx!(
    PATH_REF,
    "`((?:~/|)[A-Za-z0-9_.@-]+(?:/[A-Za-z0-9_.@-]+)+\\.[A-Za-z0-9]{1,6})`"
);
// Placeholders are illustrations, not references.
rx!(
    PLACEHOLDER,
    r"(?i-u)[<>*{}$]|(^|/)(foo|bar|baz|example|examples|sample|your|my|path|dir|some|thing|todo|xxx|name|topic|feature|date|yyyy|repo|owner|user|project|client|file|entry|title)([./-]|$)|\.\.\.|//|^https?:"
);
// A package script invocation.
//
// Only explicit run-forms. A bare `make`, `yarn` or `pnpm` matches ordinary English — "make it
// readable", "make a decision" — and produced 48 false findings on this repo's own prose.
rx!(
    SCRIPT_CALL,
    format!(
        "(?-u:\\b)(?:npm run|pnpm run|yarn run|bun run|make -C [^{JS_WS}]+)[{JS_WS}]+([A-Za-z0-9_:.-]+)"
    )
);
rx!(
    COMMON_SCRIPT,
    r"(?i-u)^(install|ci|test|start|build|dev|run|help|version|audit|init|create|exec|dlx|add|remove|why|link|publish|pack|outdated|update|upgrade|list|ls|info|login|logout|whoami|clean|all|-C)$"
);
rx!(MD_EXT, r"(?i-u)\.md$");
rx!(MD_FILE, r"(?i-u)\.(md|markdown)$");
// A relative markdown link.
fancy!(
    MD_LINK,
    format!("\\[[^\\]]*\\]\\((?!https?:|mailto:|#)([^)#{JS_WS}]+)(#[^){JS_WS}]*)?\\)")
);
fancy!(
    HEADING,
    format!("{BOL}#{{1,6}}[{JS_WS}]+([^\n\r\u{2028}\u{2029}]+)")
);
fancy!(MAKE_TARGET, format!("{BOL}([A-Za-z0-9_.-]+):"));

fn test(re: &LazyLock<Option<Regex>>, s: &str) -> bool {
    re.as_ref().is_some_and(|r| r.is_match(s))
}

/// All matches of a backtracking regex; a regex error (backtrack limit) ends the scan quietly.
fn captures_all<'t>(re: &Fancy, text: &'t str) -> Vec<fancy_regex::Captures<'t>> {
    let mut out = Vec::new();
    for caps in re.captures_iter(text) {
        match caps {
            Ok(c) => out.push(c),
            Err(_) => break,
        }
    }
    out
}

/// `path.normalize` for POSIX-style paths.
fn normalize(p: &str) -> String {
    if p.is_empty() {
        return ".".to_string();
    }
    let abs = p.starts_with('/');
    let trailing = p.ends_with('/');
    let mut segs: Vec<&str> = Vec::new();
    for s in p.split('/') {
        match s {
            "" | "." => {}
            ".." => {
                if segs.last().is_some_and(|l| *l != "..") {
                    segs.pop();
                } else if !abs {
                    segs.push("..");
                }
            }
            s => segs.push(s),
        }
    }
    let mut out = segs.join("/");
    if out.is_empty() {
        return if abs {
            "/".to_string()
        } else if trailing {
            "./".to_string()
        } else {
            ".".to_string()
        };
    }
    if trailing {
        out.push('/');
    }
    if abs {
        out.insert(0, '/');
    }
    out
}

/// `path.join(a, b)`.
fn join(a: &str, b: &str) -> String {
    let parts: Vec<&str> = [a, b].into_iter().filter(|s| !s.is_empty()).collect();
    if parts.is_empty() {
        ".".to_string()
    } else {
        normalize(&parts.join("/"))
    }
}

fn cwd() -> String {
    std::env::current_dir()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// `path.resolve(...)` of one or two segments: absolute, normalized, no trailing slash.
fn resolve(base: &str, rel: &str) -> String {
    let joined = if rel.starts_with('/') {
        rel.to_string()
    } else {
        let base = if base.starts_with('/') {
            base.to_string()
        } else if base.is_empty() {
            cwd()
        } else {
            format!("{}/{base}", cwd())
        };
        format!("{base}/{rel}")
    };
    let n = normalize(&joined);
    if n.len() > 1 {
        n.trim_end_matches('/').to_string()
    } else {
        n
    }
}

/// `path.relative(from, to)`.
fn relative(from: &str, to: &str) -> String {
    let (f, t) = (resolve(from, ""), resolve(to, ""));
    let fs_: Vec<&str> = f.split('/').filter(|s| !s.is_empty()).collect();
    let ts: Vec<&str> = t.split('/').filter(|s| !s.is_empty()).collect();
    let common = fs_.iter().zip(&ts).take_while(|(a, b)| a == b).count();
    let mut parts: Vec<&str> = vec![".."; fs_.len() - common];
    parts.extend(&ts[common..]);
    parts.join("/")
}

/// `path.dirname`.
fn dirname(p: &str) -> String {
    let b = p.as_bytes();
    if b.is_empty() {
        return ".".to_string();
    }
    let has_root = b[0] == b'/';
    let mut end = None;
    let mut matched = true;
    for i in (1..b.len()).rev() {
        if b[i] == b'/' {
            if !matched {
                end = Some(i);
                break;
            }
        } else {
            matched = false;
        }
    }
    match end {
        None => if has_root { "/" } else { "." }.to_string(),
        Some(1) if has_root => "//".to_string(),
        Some(e) => p[..e].to_string(),
    }
}

fn exists(p: &str) -> bool {
    !p.is_empty() && std::path::Path::new(p).exists()
}

/// Resolution by SUFFIX against an index of the repo's real files, not by guessing a base directory.
/// A document legitimately cites a path relative to the repo root, to a package subdirectory, or to
/// its own folder — guessing produced 28 false "stale" findings on this repo, every one a real file
/// living one directory deeper than assumed.
fn resolves(reference: &str, root: &str, doc_dir: &str, index: &[String]) -> Option<bool> {
    if test(&PLACEHOLDER, reference) {
        return None;
    }
    if let Some(rest) = reference.strip_prefix("~/") {
        let home = std::env::var_os("HOME")
            .map(|h| h.to_string_lossy().into_owned())
            .unwrap_or_default();
        return Some(exists(&join(&home, rest)));
    }
    if reference.starts_with('/') {
        return Some(exists(reference));
    }
    if [join(root, reference), join(doc_dir, reference)]
        .iter()
        .any(|p| exists(p))
    {
        return Some(true);
    }
    // Suffix matching needs enough path to be distinctive. `scripts/helper.ts` ends the same way in
    // every skill's own scripts dir, so a one-segment reference matched a sibling's file and a genuinely
    // stale path was reported as resolved — a silent false negative in the check's core purpose.
    let rel = reference.strip_prefix("./").unwrap_or(reference);
    if rel.split('/').count() < 3 {
        return Some(false);
    }
    let needle = format!("/{rel}");
    Some(index.iter().any(|p| p.ends_with(&needle)))
}

/// Entries of a directory as (name, is_dir), in byte order as libuv's `scandir` lists them.
fn entries(dir: &str) -> Option<Vec<(String, bool)>> {
    let rd = fs::read_dir(dir).ok()?;
    let mut v: Vec<(String, bool)> = rd
        .filter_map(|e| e.ok())
        .map(|e| {
            (
                e.file_name().to_string_lossy().into_owned(),
                e.file_type().is_ok_and(|t| t.is_dir()),
            )
        })
        .collect();
    v.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
    Some(v)
}

/// Every file path in the repo, once, for suffix matching.
pub fn file_index(root: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut stack = vec![root.to_string()];
    let mut visited = 0u32;
    while visited < 60_000 {
        let Some(dir) = stack.pop() else { break };
        let Some(list) = entries(&dir) else { continue };
        for (name, is_dir) in list {
            visited += 1;
            let full = join(&dir, &name);
            if is_dir {
                if !SKIP.contains(&name.as_str()) {
                    stack.push(full);
                }
            } else {
                out.push(full);
            }
        }
    }
    out
}

fn anchors_of(text: &str) -> HashSet<String> {
    let mut out = HashSet::new();
    // Renderers disambiguate repeated headings as `#config`, `#config-1`, `#config-2`. Emitting only
    // the bare slug reported those valid links as dead anchors.
    let mut seen: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let Some(re) = HEADING.as_ref() else {
        return out;
    };
    for m in captures_all(re, text) {
        let title = m.get(1).map_or("", |g| g.as_str());
        let kept: String = title
            .to_lowercase()
            .chars()
            .filter(|c| !"`*_[]()".contains(*c))
            .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-' || is_js_ws(*c))
            .collect();
        let mut slug = String::new();
        let mut in_space = false;
        for c in kept.trim_matches(is_js_ws).chars() {
            if is_js_ws(c) {
                if !in_space {
                    slug.push('-');
                }
                in_space = true;
            } else {
                in_space = false;
                slug.push(c);
            }
        }
        if slug.is_empty() {
            continue;
        }
        let n = seen.get(&slug).copied().unwrap_or(0);
        seen.insert(slug.clone(), n + 1);
        out.insert(if n == 0 { slug } else { format!("{slug}-{n}") });
    }
    out
}

pub fn audit_doc(
    file: &str,
    text: &str,
    root: &str,
    scripts: &HashSet<String>,
    index: &[String],
) -> Vec<Finding> {
    let mut findings: Vec<Finding> = Vec::new();
    let lines: Vec<&str> = text.split('\n').collect();
    // `file` arrives repo-relative, so resolving it against the process CWD pointed docDir at an
    // unrelated directory — which silently changed which references resolved. Anchor it to the root.
    let doc_dir = dirname(&if file.starts_with('/') {
        file.to_string()
    } else {
        resolve(root, file)
    });
    let finding = |line: usize, rule: &str, severity: &str, problem: String, fix: &str| Finding {
        file: file.to_string(),
        line,
        rule: rule.to_string(),
        severity: severity.to_string(),
        problem,
        fix: fix.to_string(),
    };

    // Self-calibrating, exactly as in check-memory: if NOTHING resolves, the root is wrong rather than
    // the document being entirely stale. A comparison that could not happen must not read as findings.
    struct Ref {
        reference: String,
        line: usize,
        exists: bool,
    }
    let mut refs: Vec<Ref> = Vec::new();
    if let Some(re) = PATH_REF.as_ref() {
        for (i, l) in lines.iter().enumerate() {
            for m in re.captures_iter(l) {
                let Some(g) = m.get(1) else { continue };
                let Some(r) = resolves(g.as_str(), root, &doc_dir, index) else {
                    continue;
                };
                refs.push(Ref {
                    reference: g.as_str().to_string(),
                    line: i + 1,
                    exists: r,
                });
            }
        }
    }
    let root_looks_right = refs.is_empty() || refs.iter().any(|r| r.exists);
    if !root_looks_right {
        findings.push(finding(
            refs.first().map_or(1, |r| r.line),
            "root-unknown",
            "P3",
            format!(
                "none of its {} path reference(s) resolve, so staleness could NOT be checked — the root is probably wrong",
                refs.len()
            ),
            "re-run with --root pointing at the repository this document describes",
        ));
    } else {
        for r in refs.iter().filter(|x| !x.exists) {
            findings.push(finding(
                r.line,
                "stale-path",
                "P1",
                format!(
                    "documents `{}`, which does not exist — a reader follows it, finds nothing, and stops trusting the rest",
                    r.reference
                ),
                "update it or delete the sentence; stale documentation is worse than none, because it is trusted",
            ));
        }
    }

    // Documented scripts must exist. This is the classic wrong-README: the command that works in the
    // author's shell because of something set six months ago.
    if !scripts.is_empty() {
        if let Some(re) = SCRIPT_CALL.as_ref() {
            for (i, l) in lines.iter().enumerate() {
                for m in re.captures_iter(l) {
                    let Some(name) = m.get(1).map(|g| g.as_str()) else {
                        continue;
                    };
                    if name.starts_with('-') || test(&COMMON_SCRIPT, name) {
                        continue;
                    }
                    if !scripts.contains(name) {
                        findings.push(finding(
                            i + 1,
                            "unknown-script",
                            "P1",
                            format!(
                                "documents a command `{}` but no such script is defined — a reader's first command fails",
                                &m[0]
                            ),
                            "fix the name, or add the script; run every command you document from a clean state",
                        ));
                    }
                }
            }
        }
    }

    // Relative links and their anchors.
    if let Some(re) = MD_LINK.as_ref() {
        for (i, l) in lines.iter().enumerate() {
            for m in captures_all(re, l) {
                let Some(target) = m.get(1).map(|g| g.as_str()) else {
                    continue;
                };
                let anchor = m.get(2).map(|g| g.as_str());
                if test(&PLACEHOLDER, target) {
                    continue;
                }
                let stripped = target.strip_prefix("./").unwrap_or(target);
                let resolved = [join(&doc_dir, target), join(root, target)]
                    .into_iter()
                    .find(|p| exists(p))
                    .or_else(|| {
                        let needle = format!("/{stripped}");
                        index.iter().find(|p| p.ends_with(&needle)).cloned()
                    });
                let Some(resolved) = resolved else {
                    findings.push(finding(
                        i + 1,
                        "dead-link",
                        "P1",
                        format!("links to `{target}`, which does not exist"),
                        "fix the path or remove the link",
                    ));
                    continue;
                };
                if let Some(anchor) = anchor {
                    if anchor.len() > 1 && test(&MD_EXT, &resolved) {
                        let slug = anchor[1..].to_lowercase();
                        // An unreadable target is skipped, not reported.
                        if let Ok(bytes) = fs::read(&resolved) {
                            if !anchors_of(&String::from_utf8_lossy(&bytes)).contains(&slug) {
                                findings.push(finding(
                                    i + 1,
                                    "dead-anchor",
                                    "P2",
                                    format!("links to `{target}{anchor}`, but that heading does not exist in the target"),
                                    "fix the anchor — a heading was probably renamed",
                                ));
                            }
                        }
                    }
                }
            }
        }
    }

    findings
}

fn read_scripts(root: &str) -> HashSet<String> {
    let mut out = HashSet::new();
    if let Ok(bytes) = fs::read(join(root, "package.json")) {
        let text = String::from_utf8_lossy(&bytes);
        let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
        if let Ok(pkg) = serde_json::from_str::<Value>(text) {
            // `Object.keys(pkg.scripts ?? {})`: an object's keys, and for an array or a string
            // its indices, which is what JS would list.
            match pkg.get("scripts") {
                Some(Value::Object(o)) => out.extend(o.keys().cloned()),
                Some(Value::Array(a)) => out.extend((0..a.len()).map(|i| i.to_string())),
                Some(Value::String(s)) => {
                    out.extend((0..s.encode_utf16().count()).map(|i| i.to_string()))
                }
                _ => {}
            }
        }
    }
    if let (Ok(bytes), Some(re)) = (fs::read(join(root, "Makefile")), MAKE_TARGET.as_ref()) {
        for m in captures_all(re, &String::from_utf8_lossy(&bytes)) {
            if let Some(g) = m.get(1) {
                out.insert(g.as_str().to_string());
            }
        }
    }
    out
}

fn walk(root: &str, out: &mut Vec<String>) {
    let mut stack = vec![root.to_string()];
    while let Some(dir) = stack.pop() {
        let Some(list) = entries(&dir) else { continue };
        for (name, is_dir) in list {
            let full = join(&dir, &name);
            if is_dir {
                if !SKIP.contains(&name.as_str()) {
                    stack.push(full);
                }
            } else if test(&MD_FILE, &name) {
                out.push(full);
            }
        }
    }
}

/// The base letter of a Latin-1 accented letter, which ICU sorts as that letter plus a
/// secondary difference.
fn latin_base(c: char) -> Option<char> {
    const BASES: &[(&str, char)] = &[
        ("àáâãäåÀÁÂÃÄÅ", 'a'),
        ("çÇ", 'c'),
        ("èéêëÈÉÊË", 'e'),
        ("ìíîïÌÍÎÏ", 'i'),
        ("ñÑ", 'n'),
        ("òóôõöÒÓÔÕÖ", 'o'),
        ("ùúûüÙÚÛÜ", 'u'),
        ("ýÿÝ", 'y'),
    ];
    BASES
        .iter()
        .find(|(set, _)| set.contains(c))
        .map(|(_, b)| *b)
}

/// ICU-style (primary, secondary) weight of a character; `None` is ignorable. Order (CLDR
/// root): whitespace, then `_ - , ; : ! ? . ' " ( ) [ ] { } @ * / \ & # % ` ^ + < = > | ~ $`,
/// other symbols, digits, letters. Accented Latin-1 letters sort as their base letter with a
/// secondary difference; everything else beyond ASCII goes by code point after the letters.
fn weight(c: char) -> Option<(u32, u32)> {
    const WS: &str = "\t\n\u{b}\u{c}\r ";
    const PUNCT: &str = "_-,;:!?.'\"()[]{}@*/\\&#%`^+<=>|~$";
    if let Some(i) = WS.find(c) {
        return Some((1 + i as u32, 0));
    }
    if let Some(i) = PUNCT.find(c) {
        return Some((10 + i as u32, 0));
    }
    match c {
        '0'..='9' => Some((100 + c as u32 - '0' as u32, 0)),
        'a'..='z' | 'A'..='Z' => Some((200 + c.to_ascii_lowercase() as u32 - 'a' as u32, 0)),
        c if c.is_ascii() => None,
        c => {
            let lower = c.to_lowercase().next().unwrap_or(c);
            if let Some(base) = latin_base(c) {
                return Some((200 + base as u32 - 'a' as u32, lower as u32));
            }
            if c.is_alphabetic() {
                Some((1000 + lower as u32, 0))
            } else {
                // Symbols and emoji sit between ASCII punctuation and digits.
                Some((60, c as u32))
            }
        }
    }
}

/// `a.localeCompare(b)`: primary weights first, then accents, then lowercase before uppercase.
fn locale_compare(a: &str, b: &str) -> Ordering {
    let keys = |s: &str| -> Vec<(u32, u32, bool)> {
        s.chars()
            .filter_map(|c| weight(c).map(|(p, sec)| (p, sec, c.is_uppercase())))
            .collect()
    };
    let (ka, kb) = (keys(a), keys(b));
    ka.iter()
        .map(|k| k.0)
        .cmp(kb.iter().map(|k| k.0))
        .then_with(|| ka.iter().map(|k| k.1).cmp(kb.iter().map(|k| k.1)))
        .then_with(|| ka.iter().map(|k| k.2).cmp(kb.iter().map(|k| k.2)))
}

fn severity_order(s: &str) -> u8 {
    match s {
        "P1" => 0,
        "P2" => 1,
        _ => 2,
    }
}

pub fn main(args: &[String]) -> i32 {
    let parsed = match parse_or_exit(args, &[value("root", None), flag("json"), flag("quiet")]) {
        Ok(p) => p,
        Err(code) => return code,
    };
    let (json_out, quiet) = (parsed.flag("json"), parsed.flag("quiet"));
    let root = parsed.value("root").map_or_else(cwd, str::to_string);

    let mut targets: Vec<String> = Vec::new();
    if parsed.positionals.is_empty() {
        walk(&root, &mut targets);
    } else {
        for p in &parsed.positionals {
            match fs::metadata(p) {
                Ok(m) if m.is_dir() => walk(p, &mut targets),
                Ok(_) => targets.push(p.clone()),
                Err(_) => {}
            }
        }
    }
    targets.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
    targets.dedup();
    if targets.is_empty() {
        if json_out {
            println!("{}", json!({ "scanned": 0, "findings": [] }));
        } else if !quiet {
            println!("no markdown found");
        }
        return 2;
    }

    let scripts = read_scripts(&root);
    let index = file_index(&root);
    let mut findings: Vec<Finding> = Vec::new();
    for t in &targets {
        let Ok(bytes) = fs::read(t) else { continue };
        let rel = relative(&root, t);
        let name = if rel.is_empty() { t.as_str() } else { &rel };
        findings.extend(audit_doc(
            name,
            &String::from_utf8_lossy(&bytes),
            &root,
            &scripts,
            &index,
        ));
    }

    if json_out {
        let v = json!({ "scanned": targets.len(), "findings": findings });
        match serde_json::to_string_pretty(&v) {
            Ok(s) => println!("{s}"),
            Err(e) => {
                eprintln!("error: {e}");
                return 1;
            }
        }
        return i32::from(!findings.is_empty());
    }

    findings.sort_by(|a, b| {
        severity_order(&a.severity)
            .cmp(&severity_order(&b.severity))
            .then_with(|| locale_compare(&a.file, &b.file))
    });
    for f in &findings {
        println!("{}:{}  [{} {}]", f.file, f.line, f.severity, f.rule);
        println!("    {}", f.problem);
        println!("    fix: {}", f.fix);
    }
    if !quiet {
        println!();
        if findings.is_empty() {
            println!("clean: {} document(s), references resolve", targets.len());
        } else {
            println!(
                "{} finding(s) across {} document(s)",
                findings.len(),
                targets.len()
            );
        }
        println!(
            "It resolves references; it does not RUN anything. A documented command whose script exists can\n\
             still be broken, and prose claims are not checkable at all. See /viby-toolkit:docs."
        );
    }
    i32::from(!findings.is_empty())
}
