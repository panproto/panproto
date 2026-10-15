//! `Float { bits: 16 }` reads binary16 values exactly, and a float column
//! may declare the nearest-keeping-lexeme reading of an SQL dump.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use panproto_inst::value::coerce::{LexClass, Lexeme, coerce, coerce_value};
use panproto_inst::{FidelityReason, Value, ValueSite};
use panproto_schema::value_kinds::FloatReading;
use panproto_schema::{ScalarKind, ValueSpec};

fn spec(bits: u8, float_reading: FloatReading) -> ValueSpec {
    ValueSpec {
        float_reading,
        ..ValueSpec::new(ScalarKind::Float { bits })
    }
}

fn read(text: &str, spec: &ValueSpec) -> Result<u64, FidelityReason> {
    match coerce(&Lexeme::new(text, LexClass::Number), spec).map_err(|e| e.reason)? {
        Value::Float(x) => Ok(x.to_bits()),
        other => panic!("{text}: {other:?}"),
    }
}

fn keep(x: f64, spec: &ValueSpec) -> Result<u64, FidelityReason> {
    match coerce_value(&Value::Float(x), spec, ValueSite::unknown()).map_err(|e| e.reason)? {
        Value::Float(y) => Ok(y.to_bits()),
        other => panic!("{x}: {other:?}"),
    }
}

/// The binary16 value of the bits `h`, widened.
fn widen(h: u16) -> f64 {
    panproto_expr::exact::binary16_widen(h)
}

/// The lexeme and value routes into `Float { bits: 16 }` follow law (i) at
/// 16 bits on the item's pinned cases, every one of the 65,536 binary16
/// patterns widened is kept unchanged while both binary64 neighbours of
/// each finite one are refused, and the nearest-keeping-lexeme reading
/// reads an SQL dump's texts as their nearest floats.
#[test]
fn a_float16_and_the_sql_reading_hold_their_laws() {
    let half = spec(16, FloatReading::Exact);
    let pins: [(&str, Result<f64, FidelityReason>); 9] = [
        ("0.1", Ok(0.099_975_585_937_5)),
        ("65500", Ok(65_504.0)),
        ("65504", Ok(65_504.0)),
        ("6e-8", Ok(5.960_464_477_539_063e-8)),
        ("2050", Ok(2050.0)),
        ("65519", Err(FidelityReason::PrecisionExceeded { max: 5 })),
        ("2049", Err(FidelityReason::PrecisionExceeded { max: 5 })),
        ("65520", Err(FidelityReason::Overflow)),
        ("-65520", Err(FidelityReason::Overflow)),
    ];
    for (text, expected) in pins {
        assert_eq!(read(text, &half), expected.map(f64::to_bits), "{text}");
    }
    assert_eq!(keep(0.099_975_585_937_5, &half), Ok(0.099_975_585_937_5_f64.to_bits()));
    assert_eq!(keep(f64::INFINITY, &half), Ok(f64::INFINITY.to_bits()));
    for nan in [0x7ff8_0000_0000_0000_u64, 0x7ff4_0000_0000_0000] {
        assert_eq!(keep(f64::from_bits(nan), &half), Ok(nan));
    }
    assert_eq!(
        keep(0.1, &half),
        Err(FidelityReason::PrecisionExceeded { max: 5 })
    );
    assert_eq!(
        keep(f64::from_bits(0x7ff8_0000_0000_0001), &half),
        Err(FidelityReason::NonFinite)
    );
    assert_eq!(keep(65_520.0, &half), Err(FidelityReason::Overflow));

    // Every pattern, widened, is kept; the binary64 neighbours of a finite
    // one are not binary16 values and are refused.
    let mut kept = 0_u32;
    let mut refused = 0_u32;
    for h in 0..=u16::MAX {
        let x = widen(h);
        assert_eq!(keep(x, &half), Ok(x.to_bits()), "{h:#06x}");
        kept += 1;
        if x.is_finite() {
            for neighbour in [x.to_bits().wrapping_add(1), x.to_bits().wrapping_sub(1)] {
                let y = f64::from_bits(neighbour);
                assert!(keep(y, &half).is_err(), "{h:#06x} neighbour {neighbour:#x}");
                refused += 1;
            }
        }
    }
    println!("binary16: {kept} patterns kept, {refused} binary64 neighbours refused");
    assert_eq!(kept, 65_536);
    // Two neighbours of each of the 2 x 31,744 finite patterns (the
    // binary16 exponent field all ones holds the infinities and NaNs).
    assert_eq!(refused, 2 * (2 * 31_744));

    // The nearest-keeping-lexeme reading at 64 bits, and law (4): the
    // exact reading refuses the same texts.
    let nearest = spec(64, FloatReading::NearestKeepingLexeme);
    let exact = spec(64, FloatReading::Exact);
    for (text, value, refusal) in [
        (
            "0.1000000000000000055",
            0.1,
            FidelityReason::PrecisionExceeded { max: 17 },
        ),
        (
            "12345678901234567170.0",
            12_345_678_901_234_567_168.0,
            FidelityReason::PrecisionExceeded { max: 17 },
        ),
        ("9.0e+999", f64::INFINITY, FidelityReason::Overflow),
        ("-9.0e+999", f64::NEG_INFINITY, FidelityReason::Overflow),
    ] {
        assert_eq!(read(text, &nearest), Ok(value.to_bits()), "{text}");
        assert_eq!(read(text, &exact), Err(refusal), "{text}");
    }
}
