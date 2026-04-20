//! Lex and parse a real migration field-transform expression.

use panproto_expr_parser::{parse, tokenize};

const SRC: &str = "\\record -> record.text ++ \" (alt: \" ++ record.alt ++ \")\"";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let tokens = tokenize(SRC)?;
    println!("{} tokens", tokens.len());
    let expr = parse(&tokens).map_err(|e| format!("{e:?}"))?;
    println!("parsed: {expr:?}");
    Ok(())
}
