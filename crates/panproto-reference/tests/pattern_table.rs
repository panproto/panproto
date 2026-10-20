#![allow(clippy::unwrap_used, clippy::expect_used)]
//! The pattern matcher against a hand-written table of patterns and strings.
//!
//! Every expectation below is a literal read off the `regex` crate's
//! documented syntax and its unanchored-search semantics, never a value the
//! module under test computes. Each row names a pattern, strings it must
//! match somewhere and strings it must not match anywhere; each refusal row
//! names the exact error.

mod bounded;

use panproto_reference::pattern::{PatternError, compile};

/// `(pattern, strings it matches, strings it does not match)`.
type Row = (
    &'static str,
    &'static [&'static str],
    &'static [&'static str],
);

const EMPTY_AND_LITERALS: &[Row] = &[
    ("", &["", "x", "ab"], &[]),
    ("a", &["a", "ba", "ab"], &["", "b", "A"]),
    ("ab", &["ab", "cab"], &["a", "ba", "a b"]),
    ("^ab$", &["ab"], &["cab", "abc", ""]),
    ("^a", &["a", "ab"], &["ba", ""]),
    ("a$", &["a", "ba"], &["ab", ""]),
    ("^$", &[""], &["a"]),
    ("é", &["é", "café"], &["e"]),
];

const REPETITIONS: &[Row] = &[
    ("^a*$", &["", "a", "aaa"], &["b", "ab"]),
    ("^a+$", &["a", "aa"], &["", "b", "ab"]),
    ("^ab?c$", &["ac", "abc"], &["abbc", "a", "bc"]),
    ("^a{2}$", &["aa"], &["a", "aaa"]),
    ("^a{2,}$", &["aa", "aaaa"], &["a", ""]),
    ("^a{1,2}$", &["a", "aa"], &["", "aaa"]),
    ("^a{2,2}$", &["aa"], &["a", "aaa"]),
    ("^a{0,1}$", &["", "a"], &["aa"]),
    ("^a{0}$", &[""], &["a"]),
    ("^a{10}$", &["aaaaaaaaaa"], &["aaaaaaaaa", "aaaaaaaaaaa"]),
    ("^a{2,3}$", &["aa", "aaa"], &["a", "aaaa"]),
    ("^a{0,2}b$", &["b", "ab", "aab"], &["aaab"]),
    ("^a*?$", &["", "aa"], &["b"]),
    ("^a+?b$", &["ab", "aab"], &["b"]),
    ("^a??b$", &["b", "ab"], &["aab"]),
    ("^a{1,2}?$", &["a", "aa"], &["aaa"]),
    ("^(?:ab)*$", &["", "ab", "abab"], &["aba", "a"]),
    ("^(?:ab)+$", &["ab", "abab"], &["", "aba"]),
    ("^(?:a|b)*c$", &["c", "abbac"], &["abba"]),
    ("^(?:a*)*$", &["", "aaa"], &["b"]),
    ("^(?:a?){3}$", &["", "a", "aaa"], &["aaaa"]),
    ("^a{1}{2}$", &["aa"], &["a", "aaa"]),
];

const GROUPS_AND_ALTERNATION: &[Row] = &[
    ("a|b", &["a", "b", "cb"], &["c", ""]),
    ("^(a|b)c$", &["ac", "bc"], &["cc", "abc"]),
    ("^(?:a|)$", &["", "a"], &["b"]),
    ("^(?:|b)$", &["", "b"], &["a"]),
    ("^(?:ab|cd)e$", &["abe", "cde"], &["ade", "e"]),
    ("^()$", &[""], &["a"]),
    ("^(?:)a$", &["a"], &["", "b"]),
    ("^(?P<name>a)b$", &["ab"], &["b", "aab"]),
    ("^(?<n_1>a)$", &["a"], &["b"]),
    ("^(?<ab>x)$", &["x"], &["ab"]),
    ("^(?P<ab>x)$", &["x"], &["ab"]),
    ("^((a)(b))$", &["ab"], &["a"]),
];

const ESCAPES: &[Row] = &[
    (r"^\n$", &["\n"], &["n", ""]),
    (r"^\t$", &["\t"], &["t"]),
    (r"^\r$", &["\r"], &["r", "\n"]),
    (r"^\f$", &["\u{C}"], &["f"]),
    (r"^\v$", &["\u{B}"], &["v"]),
    (r"^\a$", &["\u{7}"], &["a"]),
    (r"^\x41$", &["A"], &["x41", "a"]),
    (r"^a\x41$", &["aA"], &["a", "aAA"]),
    (r"^\x41b$", &["Ab"], &["A"]),
    (r"^\x{41}$", &["A"], &["a"]),
    (r"^a\x{41}$", &["aA"], &["a"]),
    (r"^\x{000041}$", &["A"], &["a"]),
    (r"^\x{00000041}$", &["A"], &["a"]),
    (r"^\x{000000041}$", &["A"], &["a"]),
    (r"^\x{10FFFF}$", &["\u{10FFFF}"], &["a"]),
    (r"^\x{263a}$", &["\u{263A}"], &["a"]),
    (r"^\u0041$", &["A"], &["a"]),
    (r"^a\u0041$", &["aA"], &["a"]),
    (r"^\u{263A}$", &["\u{263A}"], &["a"]),
    (r"^\U00000041$", &["A"], &["a"]),
    (r"^a\U00000041$", &["aA"], &["a"]),
    (r"^\U{1F600}$", &["\u{1F600}"], &["a"]),
    (r"^\.$", &["."], &["a"]),
    (r"^\*$", &["*"], &["a", ""]),
    (r"^\\$", &["\\"], &["a"]),
    (r"^\-$", &["-"], &["a"]),
    (r"^\#$", &["#"], &["a"]),
    (r"^\ $", &[" "], &["a"]),
    (r"^\($", &["("], &["a"]),
    (r"^\{$", &["{"], &["a"]),
    (r"^\[$", &["["], &["a"]),
    (r"^\^$", &["^"], &["a"]),
    (r"^\$$", &["$"], &["a"]),
    (r"^\|$", &["|"], &["a"]),
    (
        r"^\s$",
        &[" ", "\t", "\n", "\u{B}", "\u{A0}", "\u{3000}"],
        &["a", "", "\u{200B}"],
    ),
    (r"^\S$", &["a", "\u{200B}"], &[" ", "\t", "\u{A0}", ""]),
    (r"\Aab", &["ab", "abc"], &["cab"]),
    (r"ab\z", &["ab", "cab"], &["abc"]),
    (r"^\A$", &[""], &["a"]),
    (r"a\Ab", &[], &["ab", "a\nb"]),
];

const ANY: &[Row] = &[
    ("^.$", &["a", "é", "\r", " "], &["\n", "", "ab"]),
    ("a.c", &["abc", "xa-c"], &["ac", "a\nc"]),
    ("^.*$", &["", "abc"], &["a\nb"]),
];

const CLASSES: &[Row] = &[
    ("^[a]$", &["a"], &["b", ""]),
    ("^[abc]$", &["a", "b", "c"], &["d", "ab"]),
    ("^[azb]$", &["a", "z", "b"], &["c", "y"]),
    ("^[a-c]$", &["a", "b", "c"], &["-", "d", "`"]),
    ("^[b-c]$", &["b", "c"], &["a", "d"]),
    ("^[a-a]$", &["a"], &["b"]),
    ("^[a-]$", &["a", "-"], &["b"]),
    ("^[-a]$", &["-", "a"], &["b"]),
    ("^[a-c-]$", &["a", "b", "-"], &["d"]),
    ("^[]a]$", &["]", "a"], &["[", "b"]),
    ("^[^a]$", &["b", "]"], &["a", ""]),
    ("^[^]a]$", &["b"], &["]", "a"]),
    ("^[^a-c]$", &["d", "\n"], &["b"]),
    ("^[a&]$", &["a", "&"], &["b"]),
    ("^[&a]$", &["&", "a"], &["b"]),
    ("^[~a]$", &["~", "a"], &["b"]),
    ("^[a~]$", &["~", "a"], &["b"]),
    ("^[.]$", &["."], &["a"]),
    ("^[*+?]$", &["*", "+", "?"], &["a"]),
    (r"^[a-\x7A]$", &["a", "m", "z"], &["{", "`"]),
    (r"^[\x41-\x43]$", &["A", "B", "C"], &["D"]),
    (r"^[\x{41}-C]$", &["B"], &["D"]),
    (r"^[\n]$", &["\n"], &["n"]),
    (r"^[\t\r]$", &["\t", "\r"], &["t"]),
    (r"^[\s]$", &[" ", "\u{A0}"], &["a"]),
    (r"^[\S]$", &["a"], &[" "]),
    (r"^[\sa]$", &[" ", "a"], &["b"]),
    (r"^[^\s]$", &["a"], &[" ", "\t"]),
    (r"^[\]]$", &["]"], &["a"]),
    (r"^[\\]$", &["\\"], &["a"]),
    (r"^[\-a]$", &["-", "a"], &["b"]),
    (r"^[\^]$", &["^"], &["a"]),
    (r"^[!-\-]$", &["!", ",", "-"], &["."]),
];

const ASCII_CLASSES: &[Row] = &[
    ("^[[:alnum:]]$", &["0", "9", "A", "z"], &["_", "-", "é"]),
    ("^[[:alpha:]]$", &["a", "Z"], &["5", "_"]),
    ("^[[:ascii:]]$", &["\u{0}", "\u{7F}", "a"], &["\u{80}", "é"]),
    ("^[[:blank:]]$", &[" ", "\t"], &["\n", "a"]),
    ("^[[:cntrl:]]$", &["\u{0}", "\u{1F}", "\u{7F}"], &[" ", "a"]),
    ("^[[:digit:]]$", &["0", "9"], &["a", "/"]),
    ("^[[:graph:]]$", &["!", "~", "a"], &[" ", "\u{7F}"]),
    ("^[[:lower:]]$", &["a", "z"], &["A", "0"]),
    ("^[[:print:]]$", &[" ", "~"], &["\u{1F}", "\u{7F}"]),
    (
        "^[[:punct:]]$",
        &["!", "/", ":", "@", "[", "`", "{", "~"],
        &["a", "0", " "],
    ),
    (
        "^[[:space:]]$",
        &["\t", "\n", "\u{B}", "\u{C}", "\r", " "],
        &["a", "\u{A0}"],
    ),
    ("^[[:upper:]]$", &["A", "Z"], &["a", "0"]),
    ("^[[:word:]]$", &["0", "a", "Z", "_"], &["-", " "]),
    (
        "^[[:xdigit:]]$",
        &["0", "9", "a", "f", "A", "F"],
        &["g", "G"],
    ),
    ("^[[:digit:]x]$", &["7", "x"], &["a"]),
    ("^[x[:digit:]]$", &["7", "x"], &["a"]),
    ("^[^[:digit:]]$", &["a"], &["1"]),
    ("^[[:alpha:][:digit:]]+$", &["a1"], &["a_"]),
];

/// `(pattern, error)`, with the error written out in full.
type Refusal = (&'static str, Kind, &'static str);

#[derive(Clone, Copy, Debug)]
enum Kind {
    Invalid,
    Unsupported,
}

const REFUSALS: &[Refusal] = &[
    ("a{1", Kind::Invalid, "a malformed repetition count"),
    ("a{1,2", Kind::Invalid, "a malformed repetition count"),
    ("a{2,1}", Kind::Invalid, "a malformed repetition count"),
    ("a{3,1}", Kind::Invalid, "a malformed repetition count"),
    (
        "a{x}",
        Kind::Invalid,
        "a repetition count is not a decimal number",
    ),
    (
        "a{}",
        Kind::Invalid,
        "a repetition count is not a decimal number",
    ),
    (
        "a{,2}",
        Kind::Invalid,
        "a repetition count is not a decimal number",
    ),
    (
        "a{1,x}",
        Kind::Invalid,
        "a repetition bound is not a decimal number",
    ),
    ("*a", Kind::Invalid, "a repetition with nothing to repeat"),
    ("+", Kind::Invalid, "a repetition with nothing to repeat"),
    ("?", Kind::Invalid, "a repetition with nothing to repeat"),
    ("{1}", Kind::Invalid, "a repetition with nothing to repeat"),
    ("a|*", Kind::Invalid, "a repetition with nothing to repeat"),
    ("(?Pn>a)", Kind::Invalid, "a malformed group name"),
    ("(?<>a)", Kind::Invalid, "a malformed group name"),
    ("(?P<>a)", Kind::Invalid, "a malformed group name"),
    ("(?<a b>c)", Kind::Invalid, "a malformed group name"),
    ("(?<a", Kind::Invalid, "a malformed group name"),
    (
        "(?<=a)",
        Kind::Invalid,
        "look-behind is not supported by the syntax",
    ),
    (
        "(?<!a)",
        Kind::Invalid,
        "look-behind is not supported by the syntax",
    ),
    (
        "(?=a)",
        Kind::Invalid,
        "look-ahead is not supported by the syntax",
    ),
    (
        "(?!a)",
        Kind::Invalid,
        "look-ahead is not supported by the syntax",
    ),
    ("(?i)a", Kind::Unsupported, "inline flags"),
    ("(?i:a)", Kind::Unsupported, "inline flags"),
    ("(?s)a", Kind::Unsupported, "inline flags"),
    ("(a", Kind::Invalid, "an unclosed group"),
    ("(?:a", Kind::Invalid, "an unclosed group"),
    ("a)", Kind::Invalid, "an unopened group"),
    (")", Kind::Invalid, "an unopened group"),
    (r"\x4", Kind::Invalid, "a malformed hexadecimal escape"),
    (r"\x", Kind::Invalid, "a malformed hexadecimal escape"),
    (r"a\x4", Kind::Invalid, "a malformed hexadecimal escape"),
    (r"\u004", Kind::Invalid, "a malformed hexadecimal escape"),
    (
        r"\U0000004",
        Kind::Invalid,
        "a malformed hexadecimal escape",
    ),
    (r"\x{41", Kind::Invalid, "a malformed hexadecimal escape"),
    (r"\x{", Kind::Invalid, "a malformed hexadecimal escape"),
    (
        r"\xZZ",
        Kind::Invalid,
        "a hexadecimal escape names no character",
    ),
    (
        r"\x{}",
        Kind::Invalid,
        "a hexadecimal escape names no character",
    ),
    (
        r"\x{+41}",
        Kind::Invalid,
        "a hexadecimal escape names no character",
    ),
    (
        r"\x{-41}",
        Kind::Invalid,
        "a hexadecimal escape names no character",
    ),
    (
        r"\x{ 41}",
        Kind::Invalid,
        "a hexadecimal escape names no character",
    ),
    (
        r"\x+4",
        Kind::Invalid,
        "a hexadecimal escape names no character",
    ),
    (
        r"\u+041",
        Kind::Invalid,
        "a hexadecimal escape names no character",
    ),
    (
        r"\x{110000}",
        Kind::Invalid,
        "a hexadecimal escape names no character",
    ),
    (
        r"\x{D800}",
        Kind::Invalid,
        "a hexadecimal escape names no character",
    ),
    (
        r"\uD800",
        Kind::Invalid,
        "a hexadecimal escape names no character",
    ),
    (r"\", Kind::Invalid, "an escape at the end of the pattern"),
    (r"a\", Kind::Invalid, "an escape at the end of the pattern"),
    (r"\q", Kind::Invalid, "an unrecognized escape"),
    (r"\e", Kind::Invalid, "an unrecognized escape"),
    (r"\é", Kind::Invalid, "an unrecognized escape"),
    (r"[\A]", Kind::Invalid, "an unrecognized escape"),
    (r"[\z]", Kind::Invalid, "an unrecognized escape"),
    (r"[\b]", Kind::Invalid, "an unrecognized escape"),
    (r"[a-\z]", Kind::Invalid, "an unrecognized escape"),
    (
        r"\0",
        Kind::Invalid,
        "back-references are not supported by the syntax",
    ),
    (
        r"\1",
        Kind::Invalid,
        "back-references are not supported by the syntax",
    ),
    (
        r"(a)\1",
        Kind::Invalid,
        "back-references are not supported by the syntax",
    ),
    (
        r"\9",
        Kind::Invalid,
        "back-references are not supported by the syntax",
    ),
    (r"\d", Kind::Unsupported, "a Unicode class escape"),
    (r"\D", Kind::Unsupported, "a Unicode class escape"),
    (r"\w", Kind::Unsupported, "a Unicode class escape"),
    (r"\W", Kind::Unsupported, "a Unicode class escape"),
    (r"\pL", Kind::Unsupported, "a Unicode class escape"),
    (r"\P{L}", Kind::Unsupported, "a Unicode class escape"),
    (r"[\d]", Kind::Unsupported, "a Unicode class escape"),
    (r"\b", Kind::Unsupported, "a word boundary"),
    (r"\B", Kind::Unsupported, "a word boundary"),
    ("[a", Kind::Invalid, "an unclosed class"),
    ("[", Kind::Invalid, "an unclosed class"),
    ("[^", Kind::Invalid, "an unclosed class"),
    ("[]", Kind::Invalid, "an unclosed class"),
    ("[a-", Kind::Invalid, "an unclosed class"),
    ("[a-b", Kind::Invalid, "an unclosed class"),
    ("[[a]]", Kind::Unsupported, "a nested class"),
    ("[a[b]]", Kind::Unsupported, "a nested class"),
    ("[[:alpha]", Kind::Unsupported, "a nested class"),
    ("[[xalpha:]]", Kind::Unsupported, "a nested class"),
    ("[[:^alpha:]]", Kind::Unsupported, "a negated ASCII class"),
    ("[[:foo:]]", Kind::Unsupported, "a nested class"),
    ("[[:Alpha:]]", Kind::Unsupported, "a nested class"),
    ("[[:]]", Kind::Unsupported, "a nested class"),
    ("[a&&b]", Kind::Unsupported, "a class set operation"),
    ("[&&a]", Kind::Unsupported, "a class set operation"),
    ("[a--b]", Kind::Unsupported, "a class set operation"),
    ("[!--]", Kind::Unsupported, "a class set operation"),
    ("[a--]", Kind::Unsupported, "a class set operation"),
    ("[a-c--b]", Kind::Unsupported, "a class set operation"),
    ("[--]", Kind::Unsupported, "a class set operation"),
    ("[a~~b]", Kind::Unsupported, "a class set operation"),
    ("[~~]", Kind::Unsupported, "a class set operation"),
    (
        "[c-a]",
        Kind::Invalid,
        "a range whose end precedes its start",
    ),
    (
        "[b-a]",
        Kind::Invalid,
        "a range whose end precedes its start",
    ),
    (r"[a-\s]", Kind::Invalid, "a range ending in a class"),
    (r"[a-\S]", Kind::Invalid, "a range ending in a class"),
];

const TABLES: &[&[Row]] = &[
    EMPTY_AND_LITERALS,
    REPETITIONS,
    GROUPS_AND_ALTERNATION,
    ESCAPES,
    ANY,
    CLASSES,
    ASCII_CLASSES,
];

fn check_rows(rows: &'static [Row]) {
    for &(pattern, matching, failing) in rows {
        let compiled = bounded::within(10, move || compile(pattern))
            .unwrap_or_else(|error| panic!("{pattern:?} must compile, got {error:?}"));
        for &text in matching {
            assert!(compiled.is_match(text), "{pattern:?} must match {text:?}");
        }
        for &text in failing {
            assert!(
                !compiled.is_match(text),
                "{pattern:?} must not match {text:?}"
            );
        }
    }
}

#[test]
fn every_tabled_pattern_compiles_and_decides_its_strings() {
    bounded::within(60, || {
        for rows in TABLES {
            check_rows(rows);
        }
    });
}

#[test]
fn every_tabled_refusal_names_its_reason() {
    for &(pattern, kind, message) in REFUSALS {
        let expected = match kind {
            Kind::Invalid => PatternError::Invalid(message.to_owned()),
            Kind::Unsupported => PatternError::Unsupported(message.to_owned()),
        };
        let got = bounded::within(10, move || compile(pattern));
        assert_eq!(got, Err(expected), "refusal of {pattern:?}");
    }
}

/// Every pattern over a small alphabet of length at most three, matched
/// against every string over `{a, b}` of length at most three, agrees with
/// a test-local definition of the same language. The definitions below are
/// written per pattern shape, not derived from the module.
#[test]
fn concatenations_of_literals_and_quantifiers_agree_with_their_languages() {
    bounded::within(60, || {
        let strings = strings_over_ab(3);
        for (pattern, language) in quantified_literals() {
            let compiled = compile(&pattern).unwrap();
            for text in &strings {
                assert_eq!(
                    compiled.is_match(text),
                    language(text),
                    "{pattern:?} on {text:?}"
                );
            }
        }
    });
}

fn strings_over_ab(max_len: usize) -> Vec<String> {
    let mut out = vec![String::new()];
    let mut layer = vec![String::new()];
    for _ in 0..max_len {
        layer = layer
            .iter()
            .flat_map(|s| [format!("{s}a"), format!("{s}b")])
            .collect();
        out.extend(layer.iter().cloned());
    }
    out
}

type Language = Box<dyn Fn(&str) -> bool>;

/// `^a{min,max}$` and `^a{min,max}b$` for every bound pair up to three, with
/// the language written as a count condition on the text.
fn quantified_literals() -> Vec<(String, Language)> {
    let mut out: Vec<(String, Language)> = Vec::new();
    for min in 0..=3usize {
        for max in min..=3usize {
            let only_as = move |t: &str| t.chars().all(|c| c == 'a');
            out.push((
                format!("^a{{{min},{max}}}$"),
                Box::new(move |t: &str| only_as(t) && (min..=max).contains(&t.len())),
            ));
            out.push((
                format!("^a{{{min},{max}}}b$"),
                Box::new(move |t: &str| {
                    t.strip_suffix('b')
                        .is_some_and(|head| only_as(head) && (min..=max).contains(&head.len()))
                }),
            ));
        }
        out.push((
            format!("^a{{{min},}}$"),
            Box::new(move |t: &str| t.chars().all(|c| c == 'a') && t.len() >= min),
        ));
        out.push((
            format!("^a{{{min}}}$"),
            Box::new(move |t: &str| t.chars().all(|c| c == 'a') && t.len() == min),
        ));
    }
    out
}
