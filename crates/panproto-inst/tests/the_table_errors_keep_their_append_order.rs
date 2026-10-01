//! The table errors of `InstError` sit in one run, in the order the append
//! rule for `error.rs` fixes: `DuplicateRowId`, `HomTableMismatch`,
//! `DuplicateForeignKey`, `NotATable`, and only then `RowHasForeignKeys`.

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// The variant names of `InstError`, in declaration order.
fn inst_error_variants() -> Result<Vec<String>, String> {
    let source = include_str!("../src/error.rs");
    let start = source
        .find("pub enum InstError {")
        .ok_or("error.rs declares no InstError")?;
    let body = &source[start..];
    let end = body.find("\n}\n").ok_or("InstError has no closing brace")?;
    Ok(body[..end]
        .lines()
        .skip(1)
        .filter_map(|line| {
            let rest = line.strip_prefix("    ")?;
            if !rest.chars().next()?.is_ascii_uppercase() {
                return None;
            }
            Some(
                rest.chars()
                    .take_while(char::is_ascii_alphanumeric)
                    .collect(),
            )
        })
        .collect())
}

/// The position of `name` among `variants`.
fn position(variants: &[String], name: &str) -> Result<usize, String> {
    variants
        .iter()
        .position(|v| v == name)
        .ok_or_else(|| format!("InstError has no {name}: {variants:?}"))
}

#[test]
fn the_table_errors_form_one_run_in_the_ruled_order() -> TestResult {
    let variants = inst_error_variants()?;
    let at = position(&variants, "DuplicateRowId")?;
    assert_eq!(
        variants.get(at..at + 4),
        Some(
            &[
                "DuplicateRowId".to_owned(),
                "HomTableMismatch".to_owned(),
                "DuplicateForeignKey".to_owned(),
                "NotATable".to_owned(),
            ][..]
        ),
        "the ruled run is split: {variants:?}"
    );
    Ok(())
}

#[test]
fn the_row_move_refusal_follows_the_ruled_run() -> TestResult {
    let variants = inst_error_variants()?;
    let not_a_table = position(&variants, "NotATable")?;
    let refusal = position(&variants, "RowHasForeignKeys")?;
    assert_eq!(
        refusal,
        not_a_table + 1,
        "RowHasForeignKeys must directly follow NotATable: {variants:?}"
    );
    Ok(())
}
