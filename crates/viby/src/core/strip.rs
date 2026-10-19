//! Blank out string-literal contents, comments and regex-literal bodies while preserving every
//! line and byte offset.
//!
//! Shared because more than one checker needs it, and the alternative is matching raw text: a
//! fixture, a doc comment or a regex pattern that merely MENTIONS `it.skip` or `time.sleep` is not
//! a focused test or a sleep, and a checker that cannot tell the difference floods any repo
//! containing meta-tests.
//!
//! Port of `lib/strip-noncode.ts`. One deliberate difference: a blanked character becomes as many
//! NUL bytes as it takes in UTF-8, so a byte offset in the output is the same byte offset in the
//! input (the TypeScript kept UTF-16 offsets, which is what JavaScript strings index by).
//! String lengths are preserved too, because one caller cares whether a message string exists.

const BLANK: char = '\0';
const HASH_COMMENT_EXTS: &[&str] = &[".py", ".rb", ".sh", ".pl", ".r", ".yaml", ".yml"];
/// SQL comments with `--`, and no `/`-comments (a lone `/` there is division or a path).
const DASH_COMMENT_EXTS: &[&str] = &[".sql", ".hql", ".psql", ".lua", ".hs", ".ada"];
const TRIPLE_QUOTE_EXTS: &[&str] = &[".py", ".pyi", ".pyx"];

/// JavaScript's `\s`: Unicode white space plus the byte-order mark.
pub fn is_js_space(c: char) -> bool {
    c.is_whitespace() || c == '\u{feff}'
}

/// A `/` begins a regex literal only where a VALUE is expected. After an identifier, a closing
/// paren or bracket, or a number, it is division. The standard heuristic; it only has to avoid
/// eating real code. `prev` is the previous significant character, `None` at the start.
pub fn regex_can_start_here(prev: Option<char>) -> bool {
    match prev {
        None => true,
        Some(c) => "(,=:[!&|?{};+-*%<>~^\n".contains(c) || is_js_space(c),
    }
}

/// Index of the closing `/` of a regex literal starting at `start`, if it closes on this line.
fn find_regex_end(t: &[char], start: usize) -> Option<usize> {
    let mut in_class = false;
    let mut j = start + 1;
    while j < t.len() {
        match t[j] {
            '\n' => return None, // regex literals never span lines
            '\\' => j += 1,
            '[' => in_class = true,
            ']' => in_class = false,
            '/' if !in_class => return Some(j),
            _ => {}
        }
        j += 1;
    }
    None
}

/// Does an unescaped `delim` close on the same line, after `start`?
fn closes_on_same_line(t: &[char], start: usize, delim: char) -> bool {
    let mut j = start + 1;
    while j < t.len() {
        match t[j] {
            '\n' => return false,
            '\\' => j += 1,
            c if c == delim => return true,
            _ => {}
        }
        j += 1;
    }
    false
}

fn starts_with_at(t: &[char], i: usize, s: &[char]) -> bool {
    t.len() >= i + s.len() && &t[i..i + s.len()] == s
}

fn find_from(t: &[char], from: usize, s: &[char]) -> bool {
    (from..t.len()).any(|k| starts_with_at(t, k, s))
}

struct Out(String);

impl Out {
    fn keep(&mut self, c: char) {
        self.0.push(c);
    }

    /// Blank `c`, keeping newlines and its byte length.
    fn blank(&mut self, c: char) {
        if c == '\n' {
            self.0.push('\n');
        } else {
            for _ in 0..c.len_utf8() {
                self.0.push(BLANK);
            }
        }
    }
}

/// Blank out string-literal contents and comments for a file with extension `ext` (with the dot,
/// lower case, e.g. ".py"), preserving line and byte offsets.
pub fn strip_noncode(text: &str, ext: &str) -> String {
    let t: Vec<char> = text.chars().collect();
    let n = t.len();
    let mut out = Out(String::with_capacity(text.len()));
    let mut i = 0;
    // Previous non-whitespace character of real code, to tell a regex literal from division.
    let mut last_significant: Option<char> = None;
    // `"""` / `'''` are delimiters only in the Python family. Elsewhere three quotes are ordinary
    // characters (an escaped quote, an empty string beside a quote) and must not open a region.
    let triple_quotes = TRIPLE_QUOTE_EXTS.contains(&ext);
    let hash_comments = HASH_COMMENT_EXTS.contains(&ext);
    let dash_comments = DASH_COMMENT_EXTS.contains(&ext);
    let slash_comments = !hash_comments && !dash_comments;
    let at = |k: usize| t.get(k).copied();

    while i < n {
        let c = t[i];

        // line comments
        if (dash_comments && c == '-' && at(i + 1) == Some('-'))
            || (hash_comments && c == '#')
            || (slash_comments && c == '/' && at(i + 1) == Some('/'))
        {
            while i < n && t[i] != '\n' {
                out.blank(t[i]);
                i += 1;
            }
            continue;
        }
        // block comments
        if slash_comments && c == '/' && at(i + 1) == Some('*') {
            while i < n && !(t[i] == '*' && at(i + 1) == Some('/')) {
                out.blank(t[i]);
                i += 1;
            }
            for _ in 0..2.min(n - i) {
                out.0.push(BLANK);
            }
            i += 2;
            continue;
        }

        // Triple-quoted strings: Python family only, and only when the closing delimiter exists.
        // Unbounded, an SQL `DEFAULT ''''` opened a fake region that blanked the rest of a
        // migration, and the hazards in it vanished with total confidence.
        if triple_quotes && (c == '"' || c == '\'') {
            let delim = [c, c, c];
            if starts_with_at(&t, i, &delim) && find_from(&t, i + 3, &delim) {
                delim.iter().for_each(|&d| out.keep(d));
                i += 3;
                while i < n && !starts_with_at(&t, i, &delim) {
                    out.blank(t[i]);
                    i += 1;
                }
                for _ in 0..3.min(n.saturating_sub(i)) {
                    out.keep(c);
                }
                i += 3;
                continue;
            }
        }

        // Template literals continue across newlines until the closing backtick; `${...}` is
        // blanked with the rest.
        if c == '`' {
            out.keep(c);
            i += 1;
            while i < n && t[i] != '`' {
                if t[i] == '\\' && i + 1 < n {
                    out.blank('\\');
                    out.blank(t[i + 1]);
                    i += 2;
                    continue;
                }
                out.blank(t[i]);
                i += 1;
            }
            if i < n {
                out.keep('`');
                i += 1;
            }
            continue;
        }

        // Regex literal bodies: `getByText(/it.skip/)` is a pattern, not a focused test.
        if slash_comments && c == '/' && regex_can_start_here(last_significant) {
            if let Some(close) = find_regex_end(&t, i) {
                out.keep('/');
                t[i + 1..close].iter().for_each(|&k| out.blank(k));
                out.keep('/');
                i = close + 1;
                last_significant = Some('/');
                continue;
            }
        }

        // Quoted strings, only when the closing quote is on this line. An orphan quote is more
        // likely inside a regex (`/['"]/`) than a string, and blanking to the end of the line
        // erased real code, assertions included.
        if c == '"' || c == '\'' {
            if !closes_on_same_line(&t, i, c) {
                out.keep(c);
                i += 1;
                continue;
            }
            out.keep(c);
            i += 1;
            while i < n && t[i] != c {
                if t[i] == '\\' && i + 1 < n {
                    out.blank('\\');
                    out.blank(t[i + 1]);
                    i += 2;
                    continue;
                }
                out.blank(t[i]);
                i += 1;
            }
            if i < n {
                out.keep(c);
                i += 1;
            }
            continue;
        }

        if c == '\n' {
            last_significant = Some('\n');
        } else if !is_js_space(c) {
            last_significant = Some(c);
        }
        out.keep(c);
        i += 1;
    }
    out.0
}
