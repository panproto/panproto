//! A route's declared numeral dialect decides which lexemes are numerals
//! and which number each denotes.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use panproto_inst::value::DecimalValue;
use panproto_inst::value::coerce::{LexClass, Lexeme, coerce};
use panproto_inst::{FidelityReason, Value};
use panproto_schema::value_kinds::NumeralDialect;
use panproto_schema::{ScalarKind, ValueSpec};

const INT: ScalarKind = ScalarKind::Int {
    bits: 64,
    signed: true,
};
const FLOAT: ScalarKind = ScalarKind::Float { bits: 64 };

fn read(
    text: &str,
    class: LexClass,
    kind: ScalarKind,
    numeral: Option<NumeralDialect>,
) -> Result<Value, FidelityReason> {
    let spec = ValueSpec {
        numeral,
        ..ValueSpec::new(kind)
    };
    coerce(&Lexeme::new(text, class), &spec).map_err(|e| e.reason)
}

fn number(text: &str, kind: ScalarKind, dialect: NumeralDialect) -> Result<Value, FidelityReason> {
    read(text, LexClass::Number, kind, Some(dialect))
}

fn float_bits(result: Result<Value, FidelityReason>) -> Result<u64, FidelityReason> {
    match result? {
        Value::Float(x) => Ok(x.to_bits()),
        other => panic!("not a float: {other:?}"),
    }
}

/// The probes of the item, each read in each dialect as its table says.
#[test]
fn a_numeral_is_read_in_its_dialect() {
    use NumeralDialect as D;
    let not_a_number = Err(FidelityReason::NotANumber);
    let inf = Ok(f64::INFINITY.to_bits());
    // TOML: separators between digits, radix prefixes, a plus sign, `inf`.
    assert_eq!(number("1_000", INT, D::Toml), Ok(Value::Int(1000)));
    assert_eq!(number("0x1F", INT, D::Toml), Ok(Value::Int(31)));
    assert_eq!(float_bits(number("+inf", FLOAT, D::Toml)), inf);
    for bad in ["1__0", "_1", "1_", "0x", "+0x1", "0b2", "INF"] {
        assert_eq!(number(bad, INT, D::Toml), not_a_number, "{bad}");
    }
    // YAML 1.2: base 10 with leading zeros, `0o`, no separators.
    assert_eq!(number("017", INT, D::Yaml12), Ok(Value::Int(17)));
    assert_eq!(number("0o17", INT, D::Yaml12), Ok(Value::Int(15)));
    assert_eq!(number("1_000", INT, D::Yaml12), not_a_number);
    assert_eq!(number("0b101", INT, D::Yaml12), not_a_number);
    // YAML 1.1: a leading 0 for octal, base 60, `_` ignored, and a float
    // only with a point.
    assert_eq!(number("017", INT, D::Yaml11), Ok(Value::Int(15)));
    assert_eq!(number("22:30", INT, D::Yaml11), Ok(Value::Int(1350)));
    assert_eq!(number("1_000", INT, D::Yaml11), Ok(Value::Int(1000)));
    assert_eq!(number("1e3", FLOAT, D::Yaml11), not_a_number);
    assert_eq!(number("0b101", INT, D::Yaml11), Ok(Value::Int(5)));
    assert_eq!(
        float_bits(number("685.230_15e+03", FLOAT, D::Yaml11)),
        Ok(685_230.15_f64.to_bits())
    );
    // XSD: case-sensitive specials, `-NaN` by the recorded departure.
    assert_eq!(float_bits(number("INF", FLOAT, D::Xsd)), inf);
    assert_eq!(
        float_bits(number("-NaN", FLOAT, D::Xsd)),
        Ok(0xfff8_0000_0000_0000)
    );
    assert_eq!(number("inf", FLOAT, D::Xsd), not_a_number);
    // JSON: no plus sign, no quoted special.
    assert_eq!(number("+1", INT, D::Json), not_a_number);
    assert_eq!(
        read("NaN", LexClass::String, FLOAT, Some(D::Json)),
        not_a_number
    );
    // Java's specials.
    assert_eq!(float_bits(number("Infinity", FLOAT, D::Java)), inf);
    assert_eq!(number("inf", FLOAT, D::Java), not_a_number);
    // ProtoJSON's quoted specials and quoted exponent integers.
    assert_eq!(
        float_bits(read("NaN", LexClass::String, FLOAT, Some(D::ProtoJson))),
        Ok(0x7ff8_0000_0000_0000)
    );
    assert_eq!(
        read("1e2", LexClass::String, INT, Some(D::ProtoJson)),
        Ok(Value::Int(100))
    );
    // SQL's specials, under a float and a decimal kind.
    assert_eq!(float_bits(number("Infinity", FLOAT, D::Sql)), inf);
    assert_eq!(
        number("-infinity", ScalarKind::Decimal, D::Sql),
        Ok(Value::Decimal(DecimalValue::infinity(true)))
    );
}
