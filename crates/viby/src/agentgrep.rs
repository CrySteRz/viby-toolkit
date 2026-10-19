//! `viby agentgrep-server`: an MCP server (stdio) exposing one `agentgrep` tool. Claude Code
//! starts one per session.
//!
//! It links 1jehuang/agentgrep (MIT), the structure-aware code search JCode uses, and runs it
//! in-process the way JCode does, so the text is byte-identical to the `agentgrep` command's.
//! On top it adds the part JCode's harness adds around it: the session remembers which file
//! structures and code regions it has already been shown, and hands that to agentgrep (its
//! context file), which then shortens them in later `trace` results (the only mode that takes
//! it in 0.1.7). A file edited since is shown in full again. An `outline` is always an explicit
//! ask for one file, so it is never shortened, but what it showed counts as seen.

use crate::mcp::{self, Server, Tool, ToolResult};
use agentgrep::cli::{FindArgs, FullRegionMode, GrepArgs, OutlineArgs, SmartArgs};
use agentgrep::find::run_find;
use agentgrep::outline::run_outline;
use agentgrep::render::{
    render_find_output, render_grep_output, render_outline_output, render_smart_output,
};
use agentgrep::search::run_grep;
use agentgrep::smart_dsl::parse_smart_query;
use agentgrep::smart_engine::run_smart;
use regex::Regex;
use serde_json::{json, Map, Value};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex, OnceLock};
use std::time::{Duration, SystemTime};

/// The agentgrep release this is built against (the `agentgrep` dependency's tag).
pub const AGENTGREP_TAG: &str = "v0.1.7";
const MAX_TEXT: usize = 60_000;
const RUN_TIMEOUT: Duration = Duration::from_secs(30);
const MODES: [&str; 4] = ["grep", "find", "outline", "trace"];

pub const TOOL_DESCRIPTION: &str = "Structure-aware code search (agentgrep). mode='grep': matches grouped by file under their enclosing \
symbols. mode='find': files ranked for a topic, each with its structure. mode='outline': one file's \
symbols with line ranges, to read just the part you need. mode='trace': follow a symbol with \
terms like ['subject:parseConfig', 'relation:called_from'] and get the relevant regions in full. \
In trace, structure and regions this session was already shown, unchanged since, come back \
shortened (a 'context:' note says so); pass force=true to get them in full.";

pub fn input_schema() -> Value {
    json!({
        "type": "object",
        "required": ["mode"],
        "properties": {
            "mode": { "type": "string", "enum": MODES },
            "query": { "type": "string", "description": "grep: the text (a regex with regex=true). find: words describing what you look for." },
            "file": { "type": "string", "description": "outline: the file, relative to path." },
            "terms": {
                "type": "array",
                "items": { "type": "string" },
                "description": "trace: subject:<symbol> and relation:<defined|called_from|triggered_from|rendered|populated|comes_from|handled|implementation>, optionally support:<word> (repeatable), kind:<kind>, path:<hint>.",
            },
            "regex": { "type": "boolean", "description": "grep: treat query as a regex." },
            "path": { "type": "string", "description": "Folder to search. Default: the project." },
            "glob": { "type": "string", "description": "Only files matching this glob, e.g. 'src/**/*.ts'." },
            "type": { "type": "string", "description": "Only this file type, e.g. rust, ts, py." },
            "max_files": { "type": "integer", "minimum": 1, "description": "find (default 10) and trace (default 5)." },
            "max_regions": { "type": "integer", "minimum": 1, "description": "trace: regions to return (default 6)." },
            "paths_only": { "type": "boolean", "description": "grep, find, trace: only the matching paths." },
            "force": { "type": "boolean", "description": "trace: return everything in full, even what this session was already shown." },
        },
    })
}

/// One call, ready to run. Built from the tool input; the arguments are agentgrep's own types.
pub enum Search {
    Grep(GrepArgs),
    Find(FindArgs),
    Outline(OutlineArgs),
    Trace(SmartArgs),
}

fn need<T>(value: Option<T>, message: &str) -> Result<T, String> {
    value.ok_or_else(|| message.to_string())
}

/// A non-empty string field: JS treats `""` like a missing one.
fn text_field<'a>(input: &'a Value, key: &str) -> Option<&'a str> {
    input
        .get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
}

/// `Number.isInteger`: a JSON integer, or a float with no fraction.
fn integer_field(input: &Value, key: &str) -> Result<Option<usize>, String> {
    let Some(v) = input.get(key) else {
        return Ok(None);
    };
    let n = match (v.as_i64(), v.as_f64()) {
        (Some(n), _) => n,
        (None, Some(f)) if f.fract() == 0.0 && f.abs() < 9e15 => f as i64,
        _ => return Ok(None),
    };
    usize::try_from(n)
        .map(Some)
        .map_err(|_| format!("{key} must be at least 1"))
}

/// JS `String(x)` for the values a model might put in `terms`.
fn js_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => "null".into(),
        Value::Array(a) => a.iter().map(js_string).collect::<Vec<_>>().join(","),
        Value::Object(_) => "[object Object]".into(),
        other => other.to_string(),
    }
}

/// The agentgrep arguments for one call. A query that starts with `-` stays a query: these are
/// values, never parsed as options (the TS needed `--` for that when it ran the binary).
pub fn build_search(
    input: &Value,
    root: &Path,
    context_file: Option<&Path>,
) -> Result<Search, String> {
    let mode = input.get("mode").and_then(Value::as_str).unwrap_or("");
    let outline = mode == "outline";
    let path = Some(root.display().to_string());
    let glob = text_field(input, "glob")
        .filter(|_| !outline)
        .map(String::from);
    let file_type = text_field(input, "type")
        .filter(|_| !outline)
        .map(String::from);
    let paths_only = input.get("paths_only") == Some(&Value::Bool(true)) && !outline;
    let max_files = if mode == "find" || mode == "trace" {
        integer_field(input, "max_files")?
    } else {
        None
    };
    let max_regions = if mode == "trace" {
        integer_field(input, "max_regions")?
    } else {
        None
    };
    match mode {
        "grep" => Ok(Search::Grep(GrepArgs {
            query: need(text_field(input, "query"), "query is required for grep")?.to_string(),
            regex: input.get("regex") == Some(&Value::Bool(true)),
            file_type,
            json: false,
            paths_only,
            hidden: false,
            no_ignore: false,
            no_follow: false,
            path,
            glob,
        })),
        "find" => {
            let query = need(text_field(input, "query"), "query is required for find")?;
            let query_parts: Vec<String> = query.split_whitespace().map(String::from).collect();
            if query_parts.is_empty() {
                return Err("query is required for find".into());
            }
            Ok(Search::Find(FindArgs {
                query_parts,
                file_type,
                json: false,
                paths_only,
                debug_score: false,
                max_files: max_files.unwrap_or(10),
                hidden: false,
                no_ignore: false,
                no_follow: false,
                path,
                glob,
            }))
        }
        "outline" => Ok(Search::Outline(OutlineArgs {
            file: need(text_field(input, "file"), "file is required for outline")?.to_string(),
            json: false,
            max_items: None,
            path,
            context_json: None,
        })),
        "trace" => {
            let terms: Vec<String> = match input.get("terms") {
                Some(Value::Array(a)) => a.iter().map(js_string).collect(),
                Some(Value::String(s)) => s.split_whitespace().map(String::from).collect(),
                _ => Vec::new(),
            }
            .into_iter()
            .filter(|t| !t.is_empty())
            .collect();
            if terms.is_empty() {
                return Err("terms are required for trace, e.g. ['subject:parseConfig', 'relation:called_from']".into());
            }
            Ok(Search::Trace(SmartArgs {
                terms,
                json: false,
                max_files: max_files.unwrap_or(5),
                max_regions: max_regions.unwrap_or(6),
                full_region: FullRegionMode::Auto,
                debug_plan: false,
                debug_score: false,
                paths_only,
                path,
                file_type,
                glob,
                hidden: false,
                no_ignore: false,
                context_json: context_file.map(|p| p.display().to_string()),
            }))
        }
        _ => Err(unknown_mode(input.get("mode"))),
    }
}

fn unknown_mode(mode: Option<&Value>) -> String {
    let shown = mode.map_or_else(|| "undefined".to_string(), js_string);
    format!("Unknown mode: {shown}. Use one of {}.", MODES.join(", "))
}

/// What one result showed of a file: its structure, and which regions in full.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shown {
    pub file: String,
    pub structure: bool,
    pub regions: Vec<(usize, usize)>,
}

/// Reads agentgrep 0.1.7's text output for what it showed. Unknown output yields nothing.
pub fn shown_in(mode: &str, output: &str) -> Vec<Shown> {
    static RES: OnceLock<[Regex; 3]> = OnceLock::new();
    let [file_line, header_line, region_line] = RES.get_or_init(|| {
        [
            Regex::new(r"(?m)^file: (.+)$"),
            Regex::new(r"^[0-9]+\. (.+)$"),
            Regex::new(r"^ {5}- .* @ ([0-9]+)-([0-9]+) \([0-9]+ lines?\)$"),
        ]
        .map(|r| r.expect("static regex"))
    });
    if mode == "outline" {
        return file_line
            .captures(output)
            .map(|c| {
                vec![Shown {
                    file: c[1].to_string(),
                    structure: true,
                    regions: Vec::new(),
                }]
            })
            .unwrap_or_default();
    }
    if mode != "trace" {
        return Vec::new();
    }
    let mut shown: Vec<Shown> = Vec::new();
    let mut section = String::new();
    let mut region: Option<(usize, usize)> = None;
    for line in output.split('\n') {
        if let Some(c) = header_line.captures(line) {
            shown.push(Shown {
                file: c[1].to_string(),
                structure: false,
                regions: Vec::new(),
            });
            section.clear();
            continue;
        }
        let Some(current) = shown.last_mut() else {
            continue;
        };
        let indent = line.chars().take_while(|c| *c == ' ').count();
        if indent == 3 && line.chars().nth(3).is_some_and(|c| !c.is_whitespace()) {
            section = line.trim().to_string();
            if section == "structure:" {
                current.structure = true;
            }
            continue;
        }
        if section != "regions:" {
            continue;
        }
        if let Some(c) = region_line.captures(line) {
            region = c[1].parse().ok().zip(c[2].parse().ok());
        } else if line == "       full region:" {
            if let Some(r) = region {
                current.regions.push(r);
            }
        }
    }
    shown
}

struct Seen {
    mtime: SystemTime,
    structure: bool,
    regions: Vec<(usize, usize)>,
}

fn mtime(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

/// Node's `path.resolve`: absolute, with `.` and `..` folded away, no symlink lookups.
fn resolve(base: &Path, path: &str) -> PathBuf {
    let joined = base.join(path);
    let joined = if joined.is_absolute() {
        joined
    } else {
        std::env::current_dir().unwrap_or_default().join(joined)
    };
    let mut out = PathBuf::new();
    for part in joined.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// The confidence the context claims for what it was shown: above agentgrep's 0.7 prune bar.
const SURE: f64 = 0.95;

/// One MCP session: its memory of what it was shown. `viby agentgrep-server` makes one.
#[derive(Default)]
pub struct Session {
    /// Absolute path to what it was shown there and the file's mtime then, in first-seen order.
    seen: Mutex<Vec<(PathBuf, Seen)>>,
}

impl Session {
    pub fn new() -> Self {
        Self::default()
    }

    fn seen(&self) -> std::sync::MutexGuard<'_, Vec<(PathBuf, Seen)>> {
        self.seen.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Notes what a result showed. A file that is gone is not remembered.
    pub fn remember(&self, root: &Path, shown: &[Shown]) {
        let mut seen = self.seen();
        for s in shown {
            let path = resolve(root, &s.file);
            let Some(now) = mtime(&path) else { continue };
            let at = match seen.iter().position(|(p, _)| *p == path) {
                Some(at) if seen[at].1.mtime == now => at,
                Some(at) => {
                    seen.remove(at);
                    seen.push((path, Seen::new(now)));
                    seen.len() - 1
                }
                None => {
                    seen.push((path, Seen::new(now)));
                    seen.len() - 1
                }
            };
            let entry = &mut seen[at].1;
            entry.structure |= s.structure;
            for r in &s.regions {
                if !entry.regions.contains(r) {
                    entry.regions.push(*r);
                }
            }
        }
    }

    /// agentgrep's harness context for a search under `root`: what this session saw there, minus
    /// files changed since (they are forgotten, so they come back in full). None when there is none.
    pub fn harness_context(&self, root: &Path) -> Option<Value> {
        let mut seen = self.seen();
        seen.retain(|(path, entry)| mtime(path) == Some(entry.mtime));
        let (mut known_files, mut known_regions) = (Vec::new(), Vec::new());
        for (path, entry) in seen.iter() {
            let Ok(rel) = path.strip_prefix(root) else {
                continue;
            };
            let file = rel
                .components()
                .map(|c| c.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/");
            let sure = json!({
                "current_version_confidence": SURE,
                "prune_confidence": SURE,
                "source_strength": "returned_earlier",
                "reasons": ["returned earlier in this session, unchanged since"],
            });
            if entry.structure {
                let mut known = json!({ "path": file, "structure_confidence": SURE });
                merge(&mut known, &sure);
                known_files.push(known);
            }
            for (start_line, end_line) in &entry.regions {
                let mut known = json!({
                    "path": file, "start_line": start_line, "end_line": end_line,
                    "structure_confidence": SURE, "body_confidence": SURE,
                });
                merge(&mut known, &sure);
                known_regions.push(known);
            }
        }
        (!known_files.is_empty() || !known_regions.is_empty()).then(
            || json!({ "version": 1, "known_files": known_files, "known_regions": known_regions }),
        )
    }

    /// Runs one tool call.
    pub fn call_tool(&self, input: Value) -> ToolResult {
        let mode = input.get("mode").and_then(Value::as_str).unwrap_or("");
        if !MODES.contains(&mode) {
            return ToolResult::error(unknown_mode(input.get("mode")));
        }
        let fail = |text: String| ToolResult::error(text);
        let root = match input.get("path").and_then(Value::as_str) {
            Some(p) => resolve(&std::env::current_dir().unwrap_or_default(), p),
            None => resolve(&std::env::current_dir().unwrap_or_default(), "."),
        };
        if !root.exists() {
            return fail(format!("No such folder: {}", root.display()));
        }
        let context = (mode == "trace" && input.get("force") != Some(&Value::Bool(true)))
            .then(|| self.harness_context(&root))
            .flatten();
        let context_file = match context.map(|c| ContextFile::write(&c)) {
            Some(Ok(file)) => Some(file),
            Some(Err(err)) => return fail(format!("agentgrep {mode} failed: {err}")),
            None => None,
        };
        let search = match build_search(&input, &root, context_file.as_ref().map(|f| f.path())) {
            Ok(search) => search,
            Err(message) => return fail(message),
        };
        let output = match run_bounded(search, &root) {
            Ok(output) => output,
            Err(stderr) => {
                let stderr = stderr.trim();
                return fail(format!(
                    "agentgrep {mode} failed: {}",
                    if stderr.is_empty() { "exit 1" } else { stderr }
                ));
            }
        };
        let text = output.trim_end_matches(|c: char| c.is_whitespace() || c == '\u{feff}');
        // Only trace takes the context, so only what trace and outline show is remembered.
        if mode == "trace" || mode == "outline" {
            self.remember(&root, &shown_in(mode, text));
        }
        ToolResult::text(clip(text))
    }
}

impl Seen {
    fn new(mtime: SystemTime) -> Self {
        Self {
            mtime,
            structure: false,
            regions: Vec::new(),
        }
    }
}

fn merge(into: &mut Value, from: &Value) {
    if let (Some(into), Some(from)) = (into.as_object_mut(), from.as_object()) {
        let extra: &Map<String, Value> = from;
        into.extend(extra.iter().map(|(k, v)| (k.clone(), v.clone())));
    }
}

/// The context handed to agentgrep is read from a file; this one lives for a single call.
struct ContextFile(PathBuf);

impl ContextFile {
    fn write(context: &Value) -> Result<Self, String> {
        use std::io::Write;
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "viby-agentgrep-{}-{}.json",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
        let mut file = options.open(&path).map_err(|e| e.to_string())?;
        let guard = ContextFile(path);
        file.write_all(context.to_string().as_bytes())
            .map_err(|e| e.to_string())?;
        Ok(guard)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for ContextFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Runs agentgrep as its command does (same calls, same rendering) and returns its text, or the
/// text its command prints to stderr on failure. Bounded: a search that takes too long, or
/// panics, comes back as an error and never holds the session.
fn run_bounded(search: Search, root: &Path) -> Result<String, String> {
    let root = root.to_path_buf();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(run(search, &root));
    });
    match rx.recv_timeout(RUN_TIMEOUT) {
        Ok(result) => result,
        Err(mpsc::RecvTimeoutError::Timeout) => {
            Err(format!("timed out after {} seconds", RUN_TIMEOUT.as_secs()))
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => Err("internal error".into()),
    }
}

fn run(search: Search, root: &Path) -> Result<String, String> {
    let failed = |err: String| format!("error: {err}");
    match search {
        Search::Grep(args) => run_grep(root, &args)
            .map(|result| render_grep_output(&result, &args, None))
            .map_err(failed),
        Search::Find(args) => Ok(render_find_output(&run_find(root, &args), &args)),
        Search::Outline(args) => run_outline(root, &args)
            .map(|result| render_outline_output(&result))
            .map_err(failed),
        Search::Trace(args) => {
            let query = parse_smart_query(&args.terms).map_err(|err| {
                format!(
                    "error: {err}\n\ntrace queries use a small DSL. Example:\n  agentgrep trace subject:auth_status relation:rendered support:ui"
                )
            })?;
            run_smart(root, &query, &args)
                .map(|result| render_smart_output(&result, &args))
                .map_err(failed)
        }
    }
}

/// Cuts the text at 60 000 UTF-16 units, as the TS cut its string.
pub fn clip(text: &str) -> String {
    let length = text.encode_utf16().count();
    if length <= MAX_TEXT {
        return text.to_string();
    }
    let mut kept = String::new();
    let mut units = 0;
    for c in text.chars() {
        units += c.len_utf16();
        if units > MAX_TEXT {
            break;
        }
        kept.push(c);
    }
    format!(
        "{kept}\n… [truncated {} characters; narrow with glob, type, path or max_files]",
        length - MAX_TEXT
    )
}

pub fn main(_args: &[String]) -> i32 {
    let session = Arc::new(Session::new());
    mcp::serve(
        Server {
            name: "viby-agentgrep",
            version: env!("CARGO_PKG_VERSION").to_string(),
        },
        Tool {
            name: "agentgrep",
            description: TOOL_DESCRIPTION.to_string(),
            input_schema: input_schema(),
        },
        move |input| session.call_tool(input),
    );
    0
}
