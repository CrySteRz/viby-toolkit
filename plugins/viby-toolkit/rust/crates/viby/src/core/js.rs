//! JavaScript semantics the ports must keep: what `\s`, `trim`, `.length`, `.sort()`,
//! `localeCompare` and a regex literal mean in V8, written once for every checker.

use std::cmp::Ordering;
use std::fs;
use std::path::Path;

use fancy_regex::Regex as Fancy;
use regex::Regex;

/// JavaScript's `\s` as a regex class body, without the brackets: Rust's `\s` also matches U+0085
/// and lacks the byte-order mark.
pub const JS_SPACE: &str = r"\t\n\x0B\x0C\r \x{a0}\x{1680}\x{2000}-\x{200a}\x{2028}\x{2029}\x{202f}\x{205f}\x{3000}\x{feff}";

/// JavaScript's `.`: anything but a line terminator (Rust's `.` stops only at `\n`).
pub const JS_DOT: &str = r"[^\n\r\x{2028}\x{2029}]";

/// A JavaScript line terminator, as `^` and `$` see them under the `m` flag.
pub const JS_LINE_END: &str = r"[\n\r\x{2028}\x{2029}]";

/// JavaScript's `\s`: Unicode white space plus the byte-order mark, minus U+0085 (NEL), which
/// Rust counts as white space and JavaScript does not.
pub fn is_js_space(c: char) -> bool {
    (c.is_whitespace() && c != '\u{85}') || c == '\u{feff}'
}

/// `String.prototype.trim`.
pub fn js_trim(s: &str) -> &str {
    s.trim_matches(is_js_space)
}

/// `s.length`.
pub fn utf16_len(s: &str) -> usize {
    s.encode_utf16().count()
}

/// `s.slice(0, n)`. A cut through a surrogate pair leaves a lone surrogate, which the console
/// prints as U+FFFD; so does this.
pub fn slice_utf16(s: &str, n: usize) -> String {
    let mut out = String::new();
    let mut used = 0;
    for c in s.chars() {
        let w = c.len_utf16();
        if used + w > n {
            if used < n {
                out.push('\u{fffd}');
            }
            break;
        }
        out.push(c);
        used += w;
    }
    out
}

/// Byte offset that is `units` UTF-16 units before byte `at` of `text` (or the start), the way
/// `text.slice(Math.max(0, at - units), at)` counts. An `at` inside a character is moved back to
/// its start.
pub fn back_utf16(text: &str, at: usize, units: usize) -> usize {
    let mut used = 0;
    let mut start = at.min(text.len());
    while !text.is_char_boundary(start) {
        start -= 1;
    }
    let at = start;
    for c in text[..at].chars().rev() {
        if used >= units {
            break;
        }
        used += c.len_utf16();
        start -= c.len_utf8();
    }
    start
}

/// JavaScript's `<` on strings, and `Array.prototype.sort()` with no comparator: UTF-16 code-unit
/// order, not UTF-8 byte order.
pub fn utf16_cmp(a: &str, b: &str) -> Ordering {
    a.encode_utf16().cmp(b.encode_utf16())
}

/// `Array.prototype.sort()` with no comparator.
pub fn sort_utf16(names: &mut [String]) {
    names.sort_by_cached_key(|s| s.encode_utf16().collect::<Vec<u16>>());
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

/// `a.localeCompare(b)` in the default locale: primary weights first, then accents, then lower
/// case before upper. Checked against Node 24's ICU over 93 path-like strings: every ASCII pair
/// agrees; the misses are `ø`, `ß`, U+00A0 and a few symbols.
pub fn locale_compare(a: &str, b: &str) -> Ordering {
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

/// Rewrite a JavaScript regex source (flags: `i`, `m`) for the `regex` crate with JavaScript's
/// meaning: `\w`, `\d` and `\b` are ASCII, `\s` is JS white space, and `.` stops at every JS line
/// terminator. Class-aware, so `[^\s=]` stays one class.
pub fn js_source(pattern: &str, flags: &str) -> String {
    let mut out = String::with_capacity(pattern.len() + 16);
    if flags.contains('i') {
        out.push_str("(?i)");
    }
    if flags.contains('m') {
        out.push_str("(?m)");
    }
    let mut in_class = false;
    let mut chars = pattern.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => match chars.next() {
                Some('s') if in_class => out.push_str(JS_SPACE),
                Some('s') => out.push_str(&format!("[{JS_SPACE}]")),
                Some('S') => out.push_str(&format!("[^{JS_SPACE}]")),
                Some('w') if in_class => out.push_str("A-Za-z0-9_"),
                Some('w') => out.push_str("[A-Za-z0-9_]"),
                Some('W') => out.push_str("[^A-Za-z0-9_]"),
                Some('d') if in_class => out.push_str("0-9"),
                Some('d') => out.push_str("[0-9]"),
                Some('D') => out.push_str("[^0-9]"),
                Some('b') if !in_class => out.push_str(r"(?-u:\b)"),
                Some('B') if !in_class => out.push_str(r"(?-u:\B)"),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            },
            '[' if !in_class => {
                in_class = true;
                out.push(c);
            }
            ']' if in_class => {
                in_class = false;
                out.push(c);
            }
            '.' if !in_class => out.push_str(JS_DOT),
            _ => out.push(c),
        }
    }
    out
}

/// A compiled JavaScript pattern. Sources are constants in this crate, so a bad one is a bug and
/// panics at first use.
pub fn js_re(pattern: &str, flags: &str) -> Regex {
    Regex::new(&js_source(pattern, flags))
        .unwrap_or_else(|e| panic!("bad pattern {pattern:?}: {e}"))
}

/// As `js_re`, for the few patterns that need lookbehind.
pub fn js_fancy(pattern: &str, flags: &str) -> Fancy {
    fancy_regex::RegexBuilder::new(&js_source(pattern, flags))
        .backtrack_limit(100_000_000)
        .build()
        .unwrap_or_else(|e| panic!("bad pattern {pattern:?}: {e}"))
}

/// A lazily compiled JS regex, one per call site: `rx!("\\bDROP\\b", "i")`.
macro_rules! rx {
    ($pat:expr) => {
        rx!($pat, "")
    };
    ($pat:expr, $flags:expr) => {{
        static R: std::sync::LazyLock<regex::Regex> =
            std::sync::LazyLock::new(|| $crate::core::js::js_re($pat, $flags));
        &*R
    }};
}
pub(crate) use rx;

/// A built-in pattern already written in Rust syntax. A bad one is a bug in this crate.
pub fn static_regex(pattern: &str) -> Regex {
    Regex::new(pattern).unwrap_or_else(|e| panic!("invalid built-in pattern {pattern}: {e}"))
}

/// Every match of a lookaround pattern, stopping quietly where the backtrack limit is hit.
pub fn captures_all<'t>(re: &Fancy, text: &'t str) -> Vec<fancy_regex::Captures<'t>> {
    let mut out = Vec::new();
    for caps in re.captures_iter(text) {
        match caps {
            Ok(c) => out.push(c),
            Err(_) => break,
        }
    }
    out
}

/// `readFileSync(path, "utf8")`: invalid bytes become U+FFFD, a byte-order mark is kept; a
/// directory or an unreadable file is `None`.
pub fn read_text(path: impl AsRef<Path>) -> Option<String> {
    fs::read(path)
        .ok()
        .map(|b| String::from_utf8_lossy(&b).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The order Node's `localeCompare` (ICU, en-US) gives mixed-case, punctuated paths, ASCII and
    /// Latin-1 mixed. `ø` is left out: ICU files it after `ö`, this sorts it after the letters.
    #[test]
    fn locale_compare_orders_like_icu() {
        let icu = [
            "_x",
            "!",
            "(diff)",
            "@x",
            "#x",
            "~x",
            "$x",
            "10",
            "9",
            "a",
            "A",
            "À",
            "a b",
            "a b/c",
            "a_b",
            "a-b",
            "a!",
            "a.sql",
            "a.SQL",
            "a.ts",
            "A.ts",
            "a.ts, b.ts",
            "a/b",
            "A1",
            "ab",
            "Ab",
            "b",
            "B",
            "c",
            "ç",
            "d",
            "e",
            "E",
            "é",
            "e.sql",
            "é.sql",
            "f",
            "f.sql",
            "migrations/a.sql",
            "Migrations/a.sql",
            "migrations/B.sql",
            "n",
            "ñ",
            "o",
            "ö",
            "README.md",
            "resume",
            "Resume",
            "résume",
            "résumé",
            "src/a_b.ts",
            "src/a-b.ts",
            "src/ab.ts",
            "src/b.ts",
            "src/B.ts",
            "tests/1.ts",
            "tests/a.ts",
            "u",
            "ü",
            "x-test.ts",
            "x-y",
            "x.test.ts",
            "x.Test.ts",
            "x.y",
            "X.y",
            "x/y",
            "y",
            "ÿ",
            "z",
            "zebra",
            "Zebra",
            "Zed.rs",
        ];
        let mut shuffled: Vec<&str> = icu.iter().rev().copied().collect();
        shuffled.sort_by(|a, b| locale_compare(a, b));
        assert_eq!(shuffled, icu);
    }
}
