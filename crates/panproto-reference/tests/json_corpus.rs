#![allow(clippy::unwrap_used, clippy::expect_used)]
//! The JSON reader against a literal corpus of valid and invalid documents.
//!
//! Every expected value is written out by hand from RFC 8259's grammar; the
//! reader under test computes none of them. Valid documents carry the value
//! they denote, and invalid ones must be refused.

mod bounded;

use panproto_reference::json::{Member, Value, array_members, parse};

fn num(text: &str) -> Value {
    Value::Number(text.to_owned())
}

fn string(text: &str) -> Value {
    Value::String(text.to_owned())
}

fn scalars() -> Vec<(&'static str, Value)> {
    vec![
        ("null", Value::Null),
        ("true", Value::Bool(true)),
        ("false", Value::Bool(false)),
        (" true ", Value::Bool(true)),
        ("\t\n\r true\r\n", Value::Bool(true)),
        ("0", num("0")),
        ("-0", num("-0")),
        ("1", num("1")),
        ("-1", num("-1")),
        ("10", num("10")),
        ("1234567890", num("1234567890")),
        ("1.5", num("1.5")),
        ("-1.5", num("-1.5")),
        ("0.0", num("0.0")),
        ("0.25", num("0.25")),
        ("10.125", num("10.125")),
        ("1e5", num("1e5")),
        ("1E5", num("1E5")),
        ("1e+5", num("1e+5")),
        ("1e-5", num("1e-5")),
        ("1E-05", num("1E-05")),
        ("1.5e-5", num("1.5e-5")),
        ("0e0", num("0e0")),
        ("-0.0E-0", num("-0.0E-0")),
        ("12e34", num("12e34")),
        (" 7 ", num("7")),
    ]
}

fn strings() -> Vec<(&'static str, Value)> {
    vec![
        (r#""""#, string("")),
        (r#""a""#, string("a")),
        (r#""a b""#, string("a b")),
        (r#""\"""#, string("\"")),
        (r#""\\""#, string("\\")),
        (r#""\/""#, string("/")),
        (r#""\b""#, string("\u{8}")),
        (r#""\f""#, string("\u{C}")),
        (r#""\n""#, string("\n")),
        (r#""\r""#, string("\r")),
        (r#""\t""#, string("\t")),
        (r#""a\nb\tc""#, string("a\nb\tc")),
        (r#""\u0041""#, string("A")),
        (r#""\u00e9""#, string("é")),
        (r#""\u00E9""#, string("é")),
        (r#""\u0000""#, string("\u{0}")),
        (r#""\u001F""#, string("\u{1F}")),
        (r#""\uFFFF""#, string("\u{FFFF}")),
        (r#""\uD83D\uDE00""#, string("\u{1F600}")),
        (r#""\uD800\uDC00""#, string("\u{10000}")),
        (r#""\uDBFF\uDFFF""#, string("\u{10FFFF}")),
        ("\"é\"", string("é")),
        ("\"\u{1F600}\"", string("\u{1F600}")),
        ("\"a\u{7F}\"", string("a\u{7F}")),
        ("\"/\"", string("/")),
    ]
}

fn composites() -> Vec<(&'static str, Value)> {
    vec![
        ("[]", Value::Array(vec![])),
        ("[ ]", Value::Array(vec![])),
        ("[1]", Value::Array(vec![num("1")])),
        ("[1,2]", Value::Array(vec![num("1"), num("2")])),
        (" [ 1 , 2 ] ", Value::Array(vec![num("1"), num("2")])),
        ("[[]]", Value::Array(vec![Value::Array(vec![])])),
        (
            r#"["a",null,true,false,1.5e3]"#,
            Value::Array(vec![
                string("a"),
                Value::Null,
                Value::Bool(true),
                Value::Bool(false),
                num("1.5e3"),
            ]),
        ),
        ("{}", Value::Object(vec![])),
        ("{ }", Value::Object(vec![])),
        (
            r#"{"a":1}"#,
            Value::Object(vec![("a".to_owned(), num("1"))]),
        ),
        (
            r#"{ "a" : [true] , "b":{} }"#,
            Value::Object(vec![
                ("a".to_owned(), Value::Array(vec![Value::Bool(true)])),
                ("b".to_owned(), Value::Object(vec![])),
            ]),
        ),
        (
            r#"{"k":1,"k":2}"#,
            Value::Object(vec![("k".to_owned(), num("1")), ("k".to_owned(), num("2"))]),
        ),
        (
            r#"{"\u0041":"\n"}"#,
            Value::Object(vec![("A".to_owned(), string("\n"))]),
        ),
    ]
}

const INVALID: &[&str] = &[
    "",
    " ",
    "tru",
    "truex",
    "trUe",
    "True",
    "nul",
    "nulL",
    "fals",
    "falsy",
    "01",
    "00",
    "-",
    "+1",
    "1.",
    ".5",
    "1e",
    "1E",
    "1e+",
    "1e-",
    "1.e5",
    "--1",
    "-a",
    "1.5.5",
    "0x1",
    "1 2",
    "\u{A0}1",
    "\u{C}1",
    "1\u{C}",
    "\"",
    "\"a",
    "\"\\\"",
    r#""\x""#,
    r#""\a""#,
    r#""\'""#,
    r#""\u12""#,
    r#""\u12G4""#,
    r#""\u+123""#,
    r#""\uD800""#,
    r#""\uD800x""#,
    r#""\uD800\u0041""#,
    r#""\uD800\n""#,
    r#""\uDC00""#,
    "\"\u{1}\"",
    "\"\t\"",
    "\"\n\"",
    "\"\u{1F}\"",
    "[",
    "[1",
    "[1,]",
    "[,1]",
    "[1 2]",
    "[1;2]",
    "]",
    "{",
    "}",
    r#"{"a"}"#,
    r#"{"a":}"#,
    r#"{"a" 1}"#,
    "{a:1}",
    r#"{"a":1,}"#,
    r#"{"a":1 "b":2}"#,
    "[1]x",
    "{}x",
    "nullnull",
];

#[test]
fn every_valid_document_reads_as_its_written_value() {
    bounded::within(30, || {
        for (text, expected) in scalars().into_iter().chain(strings()).chain(composites()) {
            assert_eq!(parse(text), Some(expected), "parse {text:?}");
        }
    });
}

#[test]
fn every_invalid_document_is_refused() {
    bounded::within(30, || {
        for text in INVALID {
            assert_eq!(parse(text), None, "parse {text:?}");
        }
    });
}

fn text(t: &str) -> Member {
    Member::Text(t.to_owned())
}

fn member(s: &str) -> Member {
    Member::Str(s.to_owned())
}

#[test]
fn array_members_are_decoded_strings_or_the_text_as_written() {
    bounded::within(30, || {
        let cases: Vec<(&str, Vec<Member>)> = vec![
            ("[]", vec![]),
            (" [ ] ", vec![]),
            ("\t[\n]\r", vec![]),
            ("[1]", vec![text("1")]),
            (r#"["a"]"#, vec![member("a")]),
            (r#"[""]"#, vec![member("")]),
            (r#"["\u0041\"\\"]"#, vec![member("A\"\\")]),
            ("[ 1 , 2 ]", vec![text("1"), text("2")]),
            (
                r#"[ "a" , 1.5e3 , true , null , [1, 2] , {"k": "v"} , -0 ]"#,
                vec![
                    member("a"),
                    text("1.5e3"),
                    text("true"),
                    text("null"),
                    text("[1, 2]"),
                    text(r#"{"k": "v"}"#),
                    text("-0"),
                ],
            ),
            (r#"["a","b"]"#, vec![member("a"), member("b")]),
            (r#"[["a"]]"#, vec![text(r#"["a"]"#)]),
        ];
        for (input, expected) in cases {
            assert_eq!(array_members(input), Some(expected), "members of {input:?}");
        }
    });
}

#[test]
fn array_members_refuses_everything_but_one_array() {
    bounded::within(30, || {
        for input in [
            "",
            " ",
            "1",
            r#""a""#,
            "{}",
            "[",
            "[1",
            "[1,]",
            "[,1]",
            "[1 2]",
            r#"["a" "b"]"#,
            "[1]x",
            "[1] [2]",
            "x[1]",
            r#"["a]"#,
            r#"["\x"]"#,
            "[tru]",
            "[1.]",
            "]",
        ] {
            assert_eq!(array_members(input), None, "members of {input:?}");
        }
    });
}
