/**
 * Blank out string-literal contents, comments, and regex-literal bodies while preserving
 * every line and column offset.
 *
 * Shared because more than one tool here needs it and the alternative is matching raw text —
 * a mistake this repo has now made four separate times. A fixture, a doc comment, or a regex
 * pattern that merely MENTIONS `it.skip` or `time.sleep` is not a focused test or a sleep,
 * and a checker that cannot tell the difference floods any repo containing meta-tests.
 *
 * String lengths are preserved because at least one caller cares whether a message string
 * exists, not just that a string was there.
 */

/**
 * Does an unescaped `delim` close on the same line, starting after position `start`?
 * Used to distinguish a real string literal from a stray quote inside a regex literal.
 */
/**
 * A `/` begins a regex literal only where a VALUE is expected. After an identifier, a
 * closing paren/bracket, or a number, it is division. This is the standard disambiguation
 * heuristic; it does not need to be perfect, only to avoid eating real code.
 */
export function regexCanStartHere(prev: string): boolean {
  if (prev === "") return true;
  return "(,=:[!&|?{};+-*%<>~^\n".includes(prev) || /\s/.test(prev);
}

/** Index of the closing `/` of a regex literal starting at `start`, or -1. */
function findRegexEnd(text: string, start: number): number {
  let inClass = false;
  for (let j = start + 1; j < text.length; j += 1) {
    const ch = text[j];
    if (ch === undefined || ch === "\n") return -1; // regex literals never span lines
    if (ch === "\\") {
      j += 1;
      continue;
    }
    if (ch === "[") inClass = true;
    else if (ch === "]") inClass = false;
    else if (ch === "/" && !inClass) return j;
  }
  return -1;
}

function closesOnSameLine(text: string, start: number, delim: string): boolean {
  for (let j = start + 1; j < text.length; j += 1) {
    const ch = text[j];
    if (ch === "\n") return false;
    if (ch === "\\") {
      j += 1;
      continue;
    }
    if (ch === delim) return true;
  }
  return false;
}

const BLANK = "\x00"; // neutral filler: matches no pattern, preserves offsets
const HASH_COMMENT_EXTS = new Set([".py", ".rb", ".sh", ".pl", ".r", ".yaml", ".yml"]);

/**
 * Blank out string-literal contents and comments, preserving line and column offsets.
 *
 * The lesson is to decide on code, not on raw text. A test
 * fixture, a regex pattern, or a docstring that *mentions* `it.skip` or `time.sleep` is
 * not a focused test or a sleep — and a scanner that can't tell the difference floods
 * any repo containing meta-tests. String *lengths* are preserved because one check
 * (assertion-with-message, taught but not gated) legitimately cares whether a message
 * string exists.
 */
export function stripNoncode(text: string, ext: string): string {
  const out: string[] = [];
  let i = 0;
  // Previous non-whitespace character of real code, used to tell a regex literal from a
  // division operator.
  let lastSignificant = "";
  const n = text.length;
  const hashComments = HASH_COMMENT_EXTS.has(ext);
  const slashComments = !hashComments;
  while (i < n) {
    const c = text[i];

    // line comments
    if (hashComments && c === "#") {
      while (i < n && text[i] !== "\n") {
        out.push(BLANK);
        i += 1;
      }
      continue;
    }
    if (slashComments && c === "/" && i + 1 < n && text[i + 1] === "/") {
      while (i < n && text[i] !== "\n") {
        out.push(BLANK);
        i += 1;
      }
      continue;
    }
    // block comments
    if (slashComments && c === "/" && i + 1 < n && text[i + 1] === "*") {
      while (i < n && !(text[i] === "*" && i + 1 < n && text[i + 1] === "/")) {
        out.push(text[i] === "\n" ? "\n" : BLANK);
        i += 1;
      }
      out.push(BLANK.repeat(Math.min(2, n - i)));
      i += 2;
      continue;
    }

    // triple-quoted strings (python)
    if (c !== undefined && "\"'".includes(c) && text.slice(i, i + 3) === c.repeat(3) && (c === '"' || c === "'")) {
      const delim = text.slice(i, i + 3);
      out.push(delim);
      i += 3;
      while (i < n && text.slice(i, i + 3) !== delim) {
        out.push(text[i] === "\n" ? "\n" : BLANK);
        i += 1;
      }
      out.push(delim.slice(0, Math.max(0, Math.min(3, n - i))));
      i += 3;
      continue;
    }

    // template literals: legitimately multi-line, so — like triple-quoted strings —
    // they continue across newlines until the closing backtick. ${...} interpolation
    // is simply blanked along with the rest; we only need to not match inside it.
    if (c === "`") {
      out.push(c);
      i += 1;
      while (i < n && text[i] !== "`") {
        if (text[i] === "\\" && i + 1 < n) {
          out.push(BLANK, text[i + 1] === "\n" ? "\n" : BLANK);
          i += 2;
          continue;
        }
        out.push(text[i] === "\n" ? "\n" : BLANK);
        i += 1;
      }
      if (i < n && text[i] === "`") {
        out.push("`");
        i += 1;
      }
      continue;
    }

    // Regex literal bodies. `expect(screen.getByText(/it.skip/)).toBeTruthy()` was reported
    // as a focused-test finding because the pattern text was scanned as if it were code.
    // A `/` starts a regex only where a value is expected, which the previous significant
    // character tells us — otherwise it is division. Character classes are tracked so a
    // `/` inside `[...]` does not end the literal early.
    if (slashComments && c === "/" && regexCanStartHere(lastSignificant)) {
      const close = findRegexEnd(text, i);
      if (close > i) {
        out.push("/");
        for (let k = i + 1; k < close; k += 1) out.push(BLANK);
        out.push("/");
        i = close + 1;
        lastSignificant = "/";
        continue;
      }
    }

    // single/double quoted strings.
    //
    // Only enter string mode when a matching delimiter actually closes on THIS line.
    // A quote character with no partner is far more likely to be a quote inside a regex
    // literal (`/['"]/` — an ordinary pattern for splitting or validating quoted text)
    // than the start of a string. Blanking to end-of-line in that case erased real code:
    // `const ok = /['"]/.test(x); expect(ok).toBe(true);` lost its assertion and the test
    // was reported as assertion-free. Treating the orphan quote as an ordinary character
    // keeps the line intact, and costs nothing when it really was an unterminated string
    // (which is a syntax error the test runner will report anyway).
    if (c !== undefined && "\"'".includes(c)) {
      if (!closesOnSameLine(text, i, c)) {
        out.push(c);
        i += 1;
        continue;
      }
      const delim = c;
      out.push(delim);
      i += 1;
      while (i < n && text[i] !== delim) {
        if (text[i] === "\\" && i + 1 < n) {
          out.push(BLANK.repeat(2));
          i += 2;
          continue;
        }
        out.push(BLANK);
        i += 1;
      }
      if (i < n && text[i] === delim) {
        out.push(delim);
        i += 1;
      }
      continue;
    }

    if (c !== undefined && !/\s/.test(c)) lastSignificant = c;
    else if (c === "\n") lastSignificant = "\n";
    out.push(c ?? "");
    i += 1;
  }
  return out.join("");
}
