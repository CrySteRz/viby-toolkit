//! strip_noncode against golden data from the TypeScript original (`tests/fixtures/strip-golden.json`:
//! per input character, k = kept, b = blanked), plus the offset guarantees the Rust port adds.

use viby::core::strip::strip_noncode;

fn mask(input: &str, out: &str) -> String {
    let mut at = 0;
    input
        .chars()
        .map(|c| {
            let len = c.len_utf8();
            let kept = &out.as_bytes()[at..at + len] == c.to_string().as_bytes();
            at += len;
            if kept {
                'k'
            } else {
                'b'
            }
        })
        .collect()
}

#[test]
fn matches_the_typescript_original_character_for_character() {
    let cases: Vec<serde_json::Value> =
        serde_json::from_str(include_str!("fixtures/strip-golden.json")).unwrap();
    assert!(cases.len() >= 20);
    for case in cases {
        let (ext, input, expected) = (
            case["ext"].as_str().unwrap(),
            case["input"].as_str().unwrap(),
            case["mask"].as_str().unwrap(),
        );
        let out = strip_noncode(input, ext);
        assert_eq!(
            out.len(),
            input.len(),
            "byte length kept for {ext} {:?}",
            &input[..input.len().min(40)]
        );
        assert_eq!(
            out.matches('\n').count(),
            input.matches('\n').count(),
            "lines kept"
        );
        let got = mask(input, &out);
        if got != expected {
            let at = got
                .chars()
                .zip(expected.chars())
                .position(|(a, b)| a != b)
                .unwrap_or(0);
            let around: String = input.chars().skip(at.saturating_sub(30)).take(60).collect();
            panic!(
                "{ext}: first difference at char {at}: rust={} ts={} near {around:?}",
                got.chars().nth(at).unwrap_or('?'),
                expected.chars().nth(at).unwrap_or('?')
            );
        }
    }
}

#[test]
fn blanks_without_moving_offsets_or_lines() {
    let src = "a = \"— x\"; // é\nb = 1;\n";
    let out = strip_noncode(src, ".ts");
    assert_eq!(out.len(), src.len());
    assert_eq!(
        out.find("b = 1"),
        src.find("b = 1"),
        "a byte offset in the output is the same in the input"
    );
    assert!(!out.contains('é') && !out.contains('—'));
}

#[test]
fn an_escaped_newline_in_a_string_keeps_its_line() {
    // Intended difference from the TypeScript, which blanked the newline and shifted every later line.
    let src = "s = 'a\\\nb'; x();\n";
    let out = strip_noncode(src, ".ts");
    assert_eq!(out.matches('\n').count(), 2);
}
