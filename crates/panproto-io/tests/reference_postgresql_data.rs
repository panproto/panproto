//! The data reader against the independent reference of the
//! postgresql-data page: every bounded input text of every bounded type,
//! the rows and canonical text of every bounded script, and generated
//! longer scripts.

#![cfg(feature = "tree-sitter")]
#![allow(clippy::expect_used, clippy::unwrap_used)]

use panproto_inst::Value;
use panproto_inst::value::{DecimalClass, DecimalValue, FidelityReason};
use panproto_io::postgresql::{SqlDataError, ValueRefusal, read_dump, write_data};
use panproto_protocols::database::postgresql::{OtherStatements, SqlDdlError};
use panproto_reference::enumerate::postgresql_data::{scripts, statements, value_cases};
use panproto_reference::pages::postgresql_data::{
    Insert, Refusal, Rows, SCHEMA, ScriptRefusal, canonical_text, input_value, read_script, render,
};
use proptest::prelude::*;

/// A decimal in positional notation.
fn plain(d: &DecimalValue) -> String {
    match &d.class {
        DecimalClass::NaN { .. } => return "NaN".to_owned(),
        DecimalClass::Infinite => {
            return if d.negative { "-Infinity" } else { "Infinity" }.to_owned();
        }
        DecimalClass::Finite => {}
    }
    let scale = usize::try_from(d.exponent.negated().to_i64().unwrap()).unwrap();
    let mut digits = d.digits.clone();
    while digits.len() < scale + 1 {
        digits.insert(0, '0');
    }
    let point = digits.len() - scale;
    let sign = if d.negative { "-" } else { "" };
    if scale == 0 {
        format!("{sign}{digits}")
    } else {
        format!("{sign}{}.{}", &digits[..point], &digits[point..])
    }
}

/// A value as the reference writes it.
fn canonical(value: &Value) -> Option<String> {
    match value {
        Value::Null => None,
        Value::Int(n) => Some(format!("int:{n}")),
        Value::Decimal(d) => Some(format!("numeric:{}", plain(d))),
        Value::Bool(b) => Some(format!("bool:{b}")),
        Value::Bytes(bytes) => Some(format!(
            "bytes:{}",
            bytes.iter().map(|b| format!("{b:02x}")).collect::<String>()
        )),
        Value::Str(s) => Some(format!("text:{s}")),
        other => panic!("no reference form for {other:?}"),
    }
}

fn refusal_class(reason: &ValueRefusal) -> Refusal {
    match reason {
        ValueRefusal::Fidelity(FidelityReason::Overflow) => Refusal::Overflow,
        ValueRefusal::Fidelity(FidelityReason::ScaleExceeded { .. }) => Refusal::Scale,
        ValueRefusal::Fidelity(FidelityReason::PrecisionExceeded { .. }) => Refusal::Precision,
        ValueRefusal::Fidelity(FidelityReason::NonFinite) => Refusal::NonFinite,
        ValueRefusal::TooLong { .. } => Refusal::TooLong,
        ValueRefusal::Syntax { .. } => Refusal::Syntax,
        other => panic!("no reference class for {other:?}"),
    }
}

#[test]
fn input_texts_read_as_the_reference_on_every_bounded_case() {
    let cases = value_cases();
    assert_eq!(cases.len(), 704);
    for (ty, text) in cases {
        let src = format!(
            "CREATE TABLE t (c {ty});\nINSERT INTO t VALUES ('{}');",
            text.replace('\'', "''")
        );
        let production = match read_dump(&src, OtherStatements::Refuse) {
            Ok(reading) => Ok(canonical(&reading.instance.rows("t")[0].1["c"]).unwrap()),
            Err(SqlDataError::Value { reason, .. }) => Err(refusal_class(&reason)),
            Err(other) => panic!("{ty} {text:?}: {other}"),
        };
        assert_eq!(production, input_value(ty, text), "{ty} {text:?}");
    }
}

/// The production reading of a script of the model, as the reference
/// writes it, with the canonical text production writes.
fn production(script: &[Insert]) -> Result<(Rows, Rows, String), ScriptRefusal> {
    let src = format!("{SCHEMA}{}", render(script));
    let reading = match read_dump(&src, OtherStatements::Refuse) {
        Ok(reading) => reading,
        Err(SqlDataError::Value { .. }) => return Err(ScriptRefusal::Value),
        Err(SqlDataError::Script(SqlDdlError::Unsupported { .. })) => {
            return Err(ScriptRefusal::Unsupported);
        }
        Err(SqlDataError::Script(SqlDdlError::Syntax { .. })) => {
            return Err(ScriptRefusal::Syntax);
        }
        Err(other) => panic!("{src}: {other}"),
    };
    let rows = |table: &str, columns: &[&str]| -> Rows {
        reading
            .instance
            .rows(table)
            .into_iter()
            .map(|(_, row)| columns.iter().map(|c| canonical(&row[*c])).collect())
            .collect()
    };
    let a = rows("a", &["x", "y", "z"]);
    let b = rows("b", &["x"]);
    // Rows are numbered table by table, `a` first.
    let ids: Vec<u32> = ["a", "b"]
        .iter()
        .flat_map(|t| reading.instance.rows(t).into_iter().map(|(id, _)| id.0))
        .collect();
    assert_eq!(
        ids,
        (0..u32::try_from(ids.len()).unwrap()).collect::<Vec<_>>()
    );
    let text = write_data(&reading.schema, &reading.instance).unwrap();
    Ok((a, b, text))
}

/// Whether production reads `script` as the reference does: the same
/// rows, or the same refusal class.
fn rows_agree(script: &[Insert]) {
    let production = production(script).map(|(a, b, _)| (a, b));
    assert_eq!(production, read_script(script), "{}", render(script));
}

#[test]
fn rows_match_the_reference_on_every_bounded_script() {
    let scripts = scripts();
    assert_eq!(scripts.len(), 2060);
    let mut read = 0;
    for script in &scripts {
        rows_agree(script);
        read += usize::from(read_script(script).is_ok());
    }
    // 147 single statements and 81 pairs read; the rest are refused.
    assert_eq!(read, 228);
}

#[test]
fn canonical_text_matches_the_reference_on_every_bounded_script() {
    let mut compared = 0;
    for script in scripts() {
        let Ok((a, b)) = read_script(&script) else {
            continue;
        };
        let (_, _, text) = production(&script).unwrap();
        assert_eq!(text, canonical_text(&a, &b), "{}", render(&script));
        let reread = read_dump(&format!("{SCHEMA}{text}"), OtherStatements::Refuse).unwrap();
        let again = read_dump(
            &format!("{SCHEMA}{}", render(&script)),
            OtherStatements::Refuse,
        )
        .unwrap();
        assert_eq!(reread.instance, again.instance, "{text}");
        compared += 1;
    }
    assert_eq!(compared, 228);
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn randomized_scripts_read_as_the_reference(
        picks in prop::collection::vec(0..835_usize, 1..8)
    ) {
        let all = statements();
        let script: Vec<Insert> = picks.iter().map(|i| all[*i].clone()).collect();
        rows_agree(&script);
    }
}
