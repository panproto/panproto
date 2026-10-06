//! Exact values: decimals keep their scale and compare as numbers,
//! integers have one representation, temporals keep the offset and the
//! precision their lexeme wrote, and the expression language compares and
//! converts them without ever rounding one silently.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod exact {
    use std::cmp::Ordering;

    use panproto_expr::exact::{
        BigIntValue, CivilDate, CivilTime, DecimalValue, ExactInteger, Offset, Temporal,
    };
    use panproto_expr::{
        BuiltinOp, ExactKind, ExactParseError, ExprError, FidelityReason, Literal, apply_builtin,
    };
    use proptest::prelude::*;

    fn dec(text: &str) -> DecimalValue {
        DecimalValue::parse(text).unwrap_or_else(|e| panic!("{text}: {e}"))
    }

    const fn err(kind: ExactKind, reason: FidelityReason) -> ExactParseError {
        ExactParseError { kind, reason }
    }

    #[test]
    fn decimal_lexemes_of_json_sql_and_xsd_read_exactly() {
        let cases = [
            ("0", false, "0", 0),
            ("-0.00", false, "0", -2),
            ("1.50", false, "150", -2),
            ("1.5", false, "15", -1),
            ("+007.25", false, "725", -2),
            (".5", false, "5", -1),
            ("5.", false, "5", 0),
            ("-1e3", true, "1", 3),
            (
                "12345678901234567890.123456789",
                false,
                "12345678901234567890123456789",
                -9,
            ),
            ("1E-400", false, "1", -400),
            (
                "0.1000000000000000055511151231257827",
                false,
                "1000000000000000055511151231257827",
                -34,
            ),
        ];
        for (text, negative, digits, exponent) in cases {
            let d = dec(text);
            assert_eq!(
                (d.negative, d.digits.as_str(), d.exponent),
                (negative, digits, exponent),
                "{text}"
            );
            assert_eq!(
                DecimalValue::parse(&d.to_string()),
                Ok(d.clone()),
                "{text} re-reads"
            );
        }
        for bad in [
            "", "-", ".", "1.2.3", "1e", "e5", "abc", "1_000", " 1", "0x10",
        ] {
            assert_eq!(
                DecimalValue::parse(bad),
                Err(err(ExactKind::Decimal, FidelityReason::NotANumber)),
                "{bad:?}"
            );
        }
        for non_finite in ["NaN", "-inf", "Infinity", "+INF"] {
            assert_eq!(
                DecimalValue::parse(non_finite),
                Err(err(ExactKind::Decimal, FidelityReason::NonFinite)),
                "{non_finite}"
            );
        }
        assert_eq!(
            DecimalValue::parse("1e99999999999"),
            Err(err(ExactKind::Decimal, FidelityReason::Overflow))
        );
        assert_eq!(dec("1.50").scale(), 2);
        assert_eq!(dec("1e3").scale(), 0);
        assert_eq!(dec("1.50").minimal_scale(), 1);
        assert_eq!(dec("1.50").to_string(), "1.50");
        assert_eq!(dec("0.005").to_string(), "0.005");
        assert_eq!(dec("-1e3").to_string(), "-1e3");
        // The invariants hold for every constructor: no leading zero, no
        // negative zero.
        assert!(DecimalValue::new(false, "012", 0).is_err());
        assert!(DecimalValue::new(false, "", 0).is_err());
        assert!(!DecimalValue::new(true, "0", 4).unwrap().negative);
    }

    #[test]
    fn decimal_deserialization_keeps_the_invariants() {
        let ok: DecimalValue =
            serde_json::from_str(r#"{"negative":true,"digits":"150","exponent":-2}"#).unwrap();
        assert_eq!(ok, dec("-1.50"));
        assert!(
            serde_json::from_str::<DecimalValue>(
                r#"{"negative":false,"digits":"015","exponent":0}"#
            )
            .is_err()
        );
    }

    #[test]
    fn shortest_float_decimals_read_back_as_the_float() {
        assert_eq!(DecimalValue::from_f64_shortest(0.1), Some(dec("1e-1")));
        assert_eq!(DecimalValue::from_f64_shortest(-0.0), Some(dec("0")));
        assert_eq!(DecimalValue::from_f64_shortest(f64::NAN), None);
        assert_eq!(DecimalValue::from_f64_shortest(f64::INFINITY), None);
        for x in [0.1, 1.0 / 3.0, 1e300, 5e-324, 123_456.789, -2.5] {
            let d = DecimalValue::from_f64_shortest(x).unwrap();
            assert_eq!(d.to_f64().to_bits(), x.to_bits(), "{x}");
        }
    }

    proptest! {
        #[test]
        fn decimal_parse_keeps_scale_and_compares_numerically(
            negative in any::<bool>(),
            int_part in "[0-9]{1,25}",
            frac_part in "[0-9]{0,25}",
            trailing in 0_usize..4,
            other in -1_000_000_i64..1_000_000,
        ) {
            let sign = if negative { "-" } else { "" };
            let plain = if frac_part.is_empty() {
                format!("{sign}{int_part}")
            } else {
                format!("{sign}{int_part}.{frac_part}")
            };
            let padded_frac = format!("{frac_part}{}", "0".repeat(trailing));
            let padded = format!("{sign}{int_part}.{padded_frac}");
            let a = dec(&plain);
            let b = dec(&padded);
            // Invariants.
            prop_assert!(!a.digits.is_empty());
            prop_assert!(a.digits == "0" || !a.digits.starts_with('0'));
            prop_assert!(!(a.is_zero() && a.negative));
            prop_assert_eq!(i64::from(a.scale()), i64::try_from(frac_part.len()).unwrap());
            prop_assert_eq!(i64::from(b.scale()), i64::try_from(padded_frac.len()).unwrap());
            // Trailing zeros change the representation, never the number.
            prop_assert_eq!(a.cmp_numeric(&b), Ordering::Equal);
            prop_assert_eq!(a == b, a.scale() == b.scale());
            // Lexical form reads back to the same representation.
            prop_assert!(dec(&a.to_string()) == a);
            // Numeric comparison agrees with comparison of an integer that
            // fits both f64 and i64 exactly.
            let c = DecimalValue::from_i64(other);
            let a_int_only = dec(&format!("{sign}{int_part}"));
            let expected = if int_part.len() <= 15 {
                let x: i64 = format!("{sign}{int_part}").parse().unwrap();
                Some(x.cmp(&other))
            } else {
                None
            };
            if let Some(expected) = expected {
                prop_assert_eq!(a_int_only.cmp_numeric(&c), expected);
            }
            prop_assert_eq!(a.cmp_numeric(&c), c.cmp_numeric(&a).reverse());
        }
    }

    #[test]
    fn integers_that_fit_i64_are_always_int() {
        assert_eq!(BigIntValue::parse("0").unwrap(), ExactInteger::Int(0));
        assert_eq!(BigIntValue::parse("-0").unwrap(), ExactInteger::Int(0));
        assert_eq!(
            BigIntValue::parse("-9223372036854775808").unwrap(),
            ExactInteger::Int(i64::MIN)
        );
        assert_eq!(
            BigIntValue::parse("+0009223372036854775807").unwrap(),
            ExactInteger::Int(i64::MAX)
        );
        assert_eq!(
            BigIntValue::from_i128(i128::from(i64::MAX)),
            ExactInteger::Int(i64::MAX)
        );
        let ExactInteger::Big(above) = BigIntValue::parse("9223372036854775808").unwrap() else {
            panic!("i64::MAX + 1 does not fit");
        };
        assert_eq!(above.digits, "9223372036854775808");
        let ExactInteger::Big(below) = BigIntValue::from_i128(i128::from(i64::MIN) - 1) else {
            panic!("i64::MIN - 1 does not fit");
        };
        assert!(below.negative && below.is_well_formed());
        let ExactInteger::Big(ten_29) =
            BigIntValue::parse("100000000000000000000000000000").unwrap()
        else {
            panic!("10^29 does not fit");
        };
        assert_eq!(ten_29.cmp_numeric(&above), Ordering::Greater);
        assert_eq!(below.cmp_numeric(&above), Ordering::Less);
        assert_eq!(
            BigIntValue::from_parts(false, "00042").unwrap(),
            ExactInteger::Int(42)
        );
        assert!(BigIntValue::parse("1.0").is_err());
        assert!(BigIntValue::parse("").is_err());
        // Deserialization accepts only a well-formed out-of-range integer.
        assert!(
            serde_json::from_str::<BigIntValue>(r#"{"negative":false,"digits":"42"}"#).is_err()
        );
        assert!(
            serde_json::from_str::<BigIntValue>(
                r#"{"negative":false,"digits":"09223372036854775808"}"#
            )
            .is_err()
        );
        let big: BigIntValue =
            serde_json::from_str(r#"{"negative":false,"digits":"9223372036854775808"}"#).unwrap();
        assert_eq!(big, above);
        assert_eq!(
            Literal::integer(BigIntValue::parse("7").unwrap()),
            Literal::Int(7)
        );
    }

    #[test]
    fn temporal_offsets_and_precision_follow_the_lexeme() {
        let offset = |text: &str| Temporal::parse(text).unwrap().offset();
        assert_eq!(offset("2026-09-28T12:34:56Z"), Some(Offset::Zulu));
        assert_eq!(offset("2026-09-28T12:34:56z"), Some(Offset::Zulu));
        assert_eq!(offset("2026-09-28T12:34:56+00:00"), Some(Offset::Fixed(0)));
        assert_eq!(offset("2026-09-28T12:34:56-00:00"), Some(Offset::NegZero));
        assert_eq!(offset("2026-09-28T12:34:56"), Some(Offset::Local));
        assert_eq!(
            offset("2026-09-28 12:34:56-08:00"),
            Some(Offset::Fixed(-480))
        );
        assert_eq!(offset("12:34:56+05:30"), Some(Offset::Fixed(330)));
        assert_eq!(offset("2026-09-28"), None);

        // A leap second is accepted.
        let leap = Temporal::parse("2016-12-31T23:59:60Z").unwrap();
        let Temporal::DateTime(_, time, _) = leap else {
            panic!("a date-time")
        };
        assert_eq!(time.second, 60);

        // The written precision is kept, trailing zeros included.
        for (text, fraction) in [
            ("12:00:00", ""),
            ("12:00:00.5", "5"),
            ("12:00:00.500", "500"),
            ("12:00:00.000000001", "000000001"),
            ("12:00:00.1234567891", "1234567891"),
        ] {
            let Temporal::Time(t, Offset::Local) = Temporal::parse(text).unwrap() else {
                panic!("{text} is a local time")
            };
            assert_eq!(t.fraction.as_str(), fraction, "{text}");
            assert_eq!(Temporal::parse(text).unwrap().to_string(), text);
        }

        for text in [
            "2026-09-28T12:34:56.789+05:30",
            "2026-09-28T12:34:56-00:00",
            "2026-09-28T12:34:56+00:00",
            "2026-09-28T00:00:00Z",
            "-0044-03-15",
            "12345-01-01",
            "2024-02-29",
        ] {
            let t = Temporal::parse(text).unwrap();
            assert_eq!(t.to_string(), text);
            assert_eq!(Temporal::parse(&t.to_string()).unwrap(), t);
            let json = serde_json::to_string(&t).unwrap();
            assert_eq!(
                serde_json::from_str::<Temporal>(&json).unwrap(),
                t,
                "{json}"
            );
        }

        let invalid = err(ExactKind::Temporal, FidelityReason::InvalidTemporal);
        for bad in [
            "2023-02-29",
            "2026-13-01",
            "2026-09-31",
            "24:00:00",
            "12:60:00",
            "12:00:61",
            "12:00",
            "2026-09-28T12:00:00+24:00",
            "2026-09-28T12:00:00+0530",
            "2026-9-28",
            "2026-09-28X12:00:00",
            "12:00:00.",
        ] {
            assert_eq!(Temporal::parse(bad), Err(invalid.clone()), "{bad}");
        }
        // A fraction past nine digits is read, not refused.
        assert_eq!(
            Temporal::parse("12:00:00.1234567891").unwrap().to_string(),
            "12:00:00.1234567891"
        );
        // Deserialization checks the fields.
        assert!(serde_json::from_str::<CivilDate>(r#"{"year":2023,"month":2,"day":29}"#).is_err());
        assert!(
            serde_json::from_str::<CivilTime>(
                r#"{"hour":1,"minute":0,"second":0,"fraction":"00a"}"#
            )
            .is_err()
        );
        assert!(
            serde_json::from_str::<CivilTime>(r#"{"hour":24,"minute":0,"second":0,"fraction":""}"#)
                .is_err()
        );
        assert!(serde_json::from_str::<Offset>(r#"{"fixed":1440}"#).is_err());
        assert_eq!(
            serde_json::from_str::<Offset>(r#"{"fixed":-60}"#).unwrap(),
            Offset::Fixed(-60)
        );
    }

    #[test]
    fn temporals_compare_in_time_when_comparable() {
        let t = |s: &str| Temporal::parse(s).unwrap();
        assert_eq!(
            t("2026-09-28T12:00:00+05:30").cmp_chronological(&t("2026-09-28T06:30:00Z")),
            Some(Ordering::Equal)
        );
        assert_eq!(
            t("2026-09-28T12:00:00-00:00").cmp_chronological(&t("2026-09-28T12:00:00Z")),
            Some(Ordering::Equal)
        );
        assert_eq!(
            t("1969-12-31").cmp_chronological(&t("1970-01-01")),
            Some(Ordering::Less)
        );
        assert_eq!(
            t("2016-12-31T23:59:60Z").cmp_chronological(&t("2016-12-31T23:59:59.999Z")),
            Some(Ordering::Greater)
        );
        // Local and zoned values, or different forms, do not compare.
        assert_eq!(
            t("2026-09-28T12:00:00").cmp_chronological(&t("2026-09-28T12:00:00Z")),
            None
        );
        assert_eq!(t("2026-09-28").cmp_chronological(&t("12:00:00")), None);
        assert_eq!(CivilDate::new(1970, 1, 1).unwrap().days_since_epoch(), 0);
        assert_eq!(
            CivilDate::new(2000, 3, 1).unwrap().days_since_epoch(),
            11_017
        );
    }

    #[test]
    fn exact_literals_compare_and_convert() {
        let d = |s: &str| Literal::Decimal(dec(s));
        let big = Literal::integer(BigIntValue::parse("100000000000000000000").unwrap());
        let call = |op, args: &[Literal]| apply_builtin(op, args).unwrap();

        // Equality is representation equality.
        assert_eq!(
            call(BuiltinOp::Eq, &[d("1.50"), d("1.50")]),
            Literal::Bool(true)
        );
        assert_eq!(
            call(BuiltinOp::Eq, &[d("1.50"), d("1.5")]),
            Literal::Bool(false)
        );
        assert_eq!(
            call(BuiltinOp::Eq, &[d("1"), Literal::Int(1)]),
            Literal::Bool(false)
        );
        // Ordering is numeric and exact across int, bigint and decimal.
        assert_eq!(
            call(BuiltinOp::Lte, &[d("1.50"), d("1.5")]),
            Literal::Bool(true)
        );
        assert_eq!(
            call(BuiltinOp::Gte, &[d("1.50"), d("1.5")]),
            Literal::Bool(true)
        );
        assert_eq!(
            call(BuiltinOp::Lt, &[d("0.1"), d("0.10000000000000000001")]),
            Literal::Bool(true)
        );
        assert_eq!(
            call(BuiltinOp::Lt, &[Literal::Int(i64::MAX), big.clone()]),
            Literal::Bool(true)
        );
        assert_eq!(
            call(BuiltinOp::Gt, &[big, d("99999999999999999999.5")]),
            Literal::Bool(true)
        );
        assert_eq!(
            call(BuiltinOp::Lt, &[d("-2.5"), Literal::Int(-2)]),
            Literal::Bool(true)
        );
        let when = |s: &str| Literal::Temporal(Temporal::parse(s).unwrap());
        assert_eq!(
            call(BuiltinOp::Lt, &[when("2026-01-01"), when("2026-01-02")]),
            Literal::Bool(true)
        );
        // Incomparable pairs are typed errors, never a guess.
        for pair in [
            [d("1.5"), Literal::Float(1.5)],
            [when("2026-01-01"), d("1")],
            [when("2026-01-01T00:00:00"), when("2026-01-01T00:00:00Z")],
        ] {
            assert!(
                matches!(
                    apply_builtin(BuiltinOp::Lt, &pair),
                    Err(ExprError::TypeError { .. })
                ),
                "{pair:?}"
            );
        }

        exact_literals_convert();
    }

    /// The conversion half of `exact_literals_compare_and_convert`.
    fn exact_literals_convert() {
        let d = |s: &str| Literal::Decimal(dec(s));
        let big = Literal::integer(BigIntValue::parse("100000000000000000000").unwrap());
        let when = |s: &str| Literal::Temporal(Temporal::parse(s).unwrap());
        let call = |op, args: &[Literal]| apply_builtin(op, args).unwrap();

        assert_eq!(
            call(BuiltinOp::ToString, &[d("1.50")]),
            Literal::Str("1.50".into())
        );
        assert_eq!(
            call(BuiltinOp::ToString, std::slice::from_ref(&big)),
            Literal::Str("100000000000000000000".into())
        );
        assert_eq!(
            call(
                BuiltinOp::ToString,
                &[when("2026-09-28T12:34:56.789+05:30")]
            ),
            Literal::Str("2026-09-28T12:34:56.789+05:30".into())
        );
        assert_eq!(
            call(BuiltinOp::ToString, &[Literal::Int(3)]),
            Literal::Str("3".into())
        );
        assert_eq!(
            call(
                BuiltinOp::ParseDecimal,
                &[Literal::Str("12345678901234567890.123456789".into())]
            ),
            d("12345678901234567890.123456789")
        );
        assert_eq!(call(BuiltinOp::ParseDecimal, &[Literal::Int(-7)]), d("-7"));
        assert_eq!(
            call(BuiltinOp::ParseDecimal, std::slice::from_ref(&big)),
            d("100000000000000000000")
        );
        assert!(matches!(
            apply_builtin(BuiltinOp::ParseDecimal, &[Literal::Str("1.2.3".into())]),
            Err(ExprError::ParseError { .. })
        ));
        assert!(matches!(
            apply_builtin(BuiltinOp::ParseDecimal, &[Literal::Float(0.5)]),
            Err(ExprError::TypeError { .. })
        ));
        assert_eq!(
            call(BuiltinOp::DecimalToFloat, &[d("0.1")]),
            Literal::Float(0.1)
        );
        assert_eq!(
            call(BuiltinOp::DecimalToFloat, &[big]),
            Literal::Float(1e20)
        );
        assert_eq!(
            call(BuiltinOp::DecimalToFloat, &[d("1e400")]),
            Literal::Float(f64::INFINITY)
        );
        assert_eq!(
            call(BuiltinOp::TypeOf, &[d("1")]),
            Literal::Str("decimal".into())
        );
        assert_eq!(BuiltinOp::from_name("to_string"), Some(BuiltinOp::ToString));
        assert_eq!(
            BuiltinOp::from_name("parseDecimal"),
            Some(BuiltinOp::ParseDecimal)
        );
        assert_eq!(
            BuiltinOp::from_name("decimal_to_float"),
            Some(BuiltinOp::DecimalToFloat)
        );
    }

    #[test]
    fn arithmetic_on_exact_literals_is_a_typed_error() {
        let decimal = Literal::Decimal(dec("1.5"));
        let big = Literal::integer(BigIntValue::parse("-100000000000000000000").unwrap());
        let when = Literal::Temporal(Temporal::parse("2026-09-28").unwrap());
        for op in [
            BuiltinOp::Add,
            BuiltinOp::Sub,
            BuiltinOp::Mul,
            BuiltinOp::Div,
            BuiltinOp::Mod,
        ] {
            for (exact, name) in [(&decimal, "decimal"), (&big, "bigint"), (&when, "temporal")] {
                for args in [
                    [exact.clone(), Literal::Int(1)],
                    [Literal::Float(1.0), exact.clone()],
                ] {
                    match apply_builtin(op, &args) {
                        Err(ExprError::ExactArithmetic { op: o, operand }) => {
                            assert_eq!(o, format!("{op:?}"));
                            assert_eq!(operand, name);
                        }
                        other => panic!("{op:?} on {args:?}: {other:?}"),
                    }
                }
            }
        }
        for op in [
            BuiltinOp::Neg,
            BuiltinOp::Abs,
            BuiltinOp::Floor,
            BuiltinOp::Ceil,
            BuiltinOp::Round,
        ] {
            assert!(matches!(
                apply_builtin(op, std::slice::from_ref(&decimal)),
                Err(ExprError::ExactArithmetic { .. })
            ));
        }
        let message = apply_builtin(BuiltinOp::Add, &[decimal, Literal::Int(1)])
            .unwrap_err()
            .to_string();
        assert_eq!(
            message,
            "Add is not defined on decimal values; convert them explicitly first"
        );
    }

    #[test]
    fn exact_literals_keep_their_serde_form() {
        let lit = Literal::Decimal(dec("1.50"));
        let json = serde_json::to_string(&lit).unwrap();
        assert_eq!(
            json,
            r#"{"Decimal":{"negative":false,"digits":"150","exponent":-2}}"#
        );
        assert_eq!(serde_json::from_str::<Literal>(&json).unwrap(), lit);
        let err_json = serde_json::to_string(&err(
            ExactKind::Decimal,
            FidelityReason::PrecisionExceeded { max: 10 },
        ))
        .unwrap();
        assert_eq!(
            err_json,
            r#"{"kind":"decimal","reason":{"precision_exceeded":{"max":10}}}"#
        );
    }

    /// The canonical bytes of a temporal, written here from the layout
    /// without calling the encoder: a form byte, the date (year as a
    /// little-endian `i64`, month, day), the time (hour, minute, second,
    /// the fraction's digit count as a little-endian `u64`, its ASCII
    /// digits) and the offset tag (0 local, 1 `Z`, 2 fixed with its minutes
    /// as a little-endian `i64`, 3 `-00:00`).
    fn layout(
        date: Option<(i64, u8, u8)>,
        time: Option<(u8, u8, u8, &str)>,
        offset: Option<i64>,
        offset_tag: u8,
    ) -> Vec<u8> {
        let mut out = vec![match (date, time) {
            (Some(_), None) => 0,
            (None, Some(_)) => 1,
            _ => 2,
        }];
        if let Some((year, month, day)) = date {
            out.extend_from_slice(&year.to_le_bytes());
            out.extend_from_slice(&[month, day]);
        }
        if let Some((hour, minute, second, fraction)) = time {
            out.extend_from_slice(&[hour, minute, second]);
            out.extend_from_slice(&u64::try_from(fraction.len()).unwrap().to_le_bytes());
            out.extend_from_slice(fraction.as_bytes());
            out.push(offset_tag);
            if let Some(minutes) = offset {
                out.extend_from_slice(&minutes.to_le_bytes());
            }
        }
        out
    }

    #[test]
    fn temporal_canonical_bytes_follow_the_layout() {
        let digits = "123456789012345678901234567890";
        let offsets: [(&str, u8, Option<i64>); 6] = [
            ("", 0, None),
            ("Z", 1, None),
            ("+00:00", 2, Some(0)),
            ("-08:00", 2, Some(-480)),
            ("+23:59", 2, Some(1439)),
            ("-00:00", 3, None),
        ];
        for (written, tag, minutes) in offsets {
            for n in [0, 3, 9, 10, 30] {
                let fraction = &digits[..n];
                let dot = if n == 0 { "" } else { "." };
                let time = format!("07:08:60{dot}{fraction}{written}");
                assert_eq!(
                    Temporal::parse(&time).unwrap().canonical_bytes(),
                    layout(None, Some((7, 8, 60, fraction)), minutes, tag),
                    "{time}"
                );
                let date_time = format!("-0044-03-15T07:08:09{dot}{fraction}{written}");
                assert_eq!(
                    Temporal::parse(&date_time).unwrap().canonical_bytes(),
                    layout(Some((-44, 3, 15)), Some((7, 8, 9, fraction)), minutes, tag),
                    "{date_time}"
                );
            }
        }
        assert_eq!(
            Temporal::parse("2026-09-28").unwrap().canonical_bytes(),
            layout(Some((2026, 9, 28)), None, None, 0)
        );
        // The written precision reaches the bytes.
        let bytes = |t: &str| Temporal::parse(t).unwrap().canonical_bytes();
        assert_ne!(bytes("12:00:00.1"), bytes("12:00:00.10"));
        assert_ne!(bytes("12:00:00"), bytes("12:00:00.0"));
        // The three zero offsets have three encodings.
        let zeros = ["12:00:00Z", "12:00:00+00:00", "12:00:00-00:00"].map(bytes);
        assert!(zeros[0] != zeros[1] && zeros[1] != zeros[2] && zeros[0] != zeros[2]);
    }

    #[test]
    fn temporal_serde_refuses_the_former_time_shape() {
        let former = r#"{"hour":1,"minute":0,"second":0,"nanos":500000000,"frac_digits":1}"#;
        let error = serde_json::from_str::<CivilTime>(former)
            .unwrap_err()
            .to_string();
        assert!(error.contains("missing field `fraction`"), "{error}");
        let time: CivilTime =
            serde_json::from_str(r#"{"hour":1,"minute":0,"second":0,"fraction":"50"}"#).unwrap();
        assert_eq!(time.fraction.as_str(), "50");
        assert_eq!(
            serde_json::to_string(&time).unwrap(),
            r#"{"hour":1,"minute":0,"second":0,"fraction":"50"}"#
        );
        for bad in ["5.0", "-5", "5 ", "\\u0665"] {
            let json = format!(r#"{{"hour":1,"minute":0,"second":0,"fraction":"{bad}"}}"#);
            assert!(serde_json::from_str::<CivilTime>(&json).is_err(), "{json}");
        }
        let digits = "123456789012345678901234567890";
        for n in [0, 3, 9, 10, 30] {
            let dot = if n == 0 { "" } else { "." };
            for text in [
                format!("23:59:60{dot}{}", &digits[..n]),
                format!("2026-09-28T00:00:00{dot}{}-00:00", &digits[..n]),
            ] {
                let t = Temporal::parse(&text).unwrap();
                let json = serde_json::to_string(&t).unwrap();
                assert_eq!(
                    serde_json::from_str::<Temporal>(&json).unwrap(),
                    t,
                    "{json}"
                );
            }
        }
    }

    #[test]
    fn a_time_keeps_every_fractional_digit() {
        let t = |s: &str| Temporal::parse(s).unwrap();
        for text in [
            "12:00:00.1234567891",
            "12:00:00.123456789012345678901234567890",
            "2026-09-28T12:00:00.1234567890Z",
            "2026-09-28T12:00:00.000000000000000000000000000001+05:30",
        ] {
            assert_eq!(t(text).to_string(), text);
            assert_eq!(t(&t(text).to_string()), t(text));
        }
        // Representation equality keeps the written precision; the
        // chronological comparison compares the instants.
        assert_ne!(t("12:00:00.1"), t("12:00:00.10"));
        assert_ne!(t("12:00:00.1234567890"), t("12:00:00.123456789"));
        for (a, b) in [
            ("12:00:00.1", "12:00:00.10"),
            ("12:00:00.123456789", "12:00:00.1234567890"),
            ("12:00:00", "12:00:00.000000000000000000000000000000"),
        ] {
            assert_eq!(
                t(a).cmp_chronological(&t(b)),
                Some(Ordering::Equal),
                "{a} {b}"
            );
        }
        assert_eq!(
            t("12:00:00.1234567891").cmp_chronological(&t("12:00:00.123456789")),
            Some(Ordering::Greater)
        );
        assert_eq!(
            t("12:00:00.0000000000000000000000000000001").cmp_chronological(&t("12:00:00")),
            Some(Ordering::Greater)
        );
    }

    /// Every fraction over the digits 0, 1 and 9 of at most four digits.
    fn small_fractions() -> Vec<String> {
        let mut all = vec![String::new()];
        let mut layer = vec![String::new()];
        for _ in 0..4 {
            layer = layer
                .iter()
                .flat_map(|f| ["0", "1", "9"].map(|d| format!("{f}{d}")))
                .collect();
            all.extend(layer.iter().cloned());
        }
        all
    }

    /// The instant of `seconds` past midnight plus `0.fraction`, in units of
    /// 10^-4 seconds (exact for fractions of at most four digits).
    fn ten_thousandths(seconds: i64, fraction: &str) -> i64 {
        let padded = format!("{fraction:0<4}");
        seconds * 10_000 + padded.parse::<i64>().unwrap()
    }

    /// The chronological comparison against an oracle on every pair of times
    /// in a bounded scope: seconds 58 to 60 of one minute, the fractions of
    /// [`small_fractions`], and the offsets `Z`, `+00:00`, `-00:00` and
    /// `+00:01` (one minute east, so 12:01:xx there is 12:00:xx in UTC).
    #[test]
    fn chronological_order_agrees_with_an_oracle_on_a_bounded_scope() {
        let fractions = small_fractions();
        assert_eq!(fractions.len(), 121);
        let offsets = [("Z", 0), ("+00:00", 0), ("-00:00", 0), ("+00:01", 1)];
        let mut times = Vec::new();
        for second in 58..=60 {
            for fraction in &fractions {
                for (written, minutes) in offsets {
                    let dot = if fraction.is_empty() { "" } else { "." };
                    let text = format!("12:{minutes:02}:{second:02}{dot}{fraction}{written}");
                    let instant = ten_thousandths(12 * 3600 + i64::from(second), fraction);
                    times.push((Temporal::parse(&text).unwrap(), instant));
                }
            }
        }
        let mut checked = 0_u64;
        for (a, ia) in &times {
            for (b, ib) in &times {
                assert_eq!(a.cmp_chronological(b), Some(ia.cmp(ib)), "{a} {b}");
                checked += 1;
            }
        }
        assert_eq!(checked, 1452 * 1452);
    }

    /// The shortest text of a float breaks an exact tie between two
    /// candidates of equal length away from zero, and keeps the scale-free
    /// form of the digits.
    #[test]
    fn shortest_text_breaks_a_tie_away_from_zero() {
        // 2229016984128891.25 lies exactly between the 17-digit decimals
        // 2229016984128891.2 and 2229016984128891.3, both of which read back
        // to it.
        let x = f64::from_bits(0x431f_ad1d_d626_35ed);
        assert_eq!(format!("{x:.2}"), "2229016984128891.25");
        for text in ["2229016984128891.2", "2229016984128891.3"] {
            assert_eq!(text.parse::<f64>().unwrap().to_bits(), x.to_bits());
        }
        assert_eq!(
            DecimalValue::from_f64_shortest(x).unwrap(),
            dec("2229016984128891.3")
        );
        assert_eq!(
            DecimalValue::from_f64_shortest(-x).unwrap(),
            dec("-2229016984128891.3")
        );
        // A positive power of ten is kept as an exponent.
        let thousand = dec("1e3");
        assert_eq!(
            (
                thousand.digits.as_str(),
                thousand.exponent,
                thousand.scale()
            ),
            ("1", 3, 0)
        );
    }

    /// Date-times across day, month, year and leap-day boundaries, at the
    /// three zero offsets and two others, compared on every pair against
    /// instants computed here from day numbers written out by hand.
    #[test]
    fn chronological_order_spans_days_on_a_bounded_scope() {
        // Days since 1970-01-01, counted by hand.
        let days = [
            ("1969-12-31", -1_i64),
            ("1970-01-01", 0),
            ("1970-01-02", 1),
            ("2024-02-28", 19_781),
            ("2024-02-29", 19_782),
            ("2024-03-01", 19_783),
            ("2026-12-31", 20_818),
            ("2027-01-01", 20_819),
        ];
        let offsets = [
            ("Z", 0_i64),
            ("+00:00", 0),
            ("-00:00", 0),
            ("+05:30", 330),
            ("-08:00", -480),
        ];
        let times = [
            ("00:00:00", 0_i64),
            ("12:00:00.5", 43_200),
            ("23:59:59.9", 86_399),
        ];
        let mut all = Vec::new();
        for (date, day) in days {
            for (time, seconds) in times {
                for (written, minutes) in offsets {
                    let tenths = if time.ends_with(".5") {
                        5
                    } else if time.ends_with(".9") {
                        9
                    } else {
                        0
                    };
                    let instant = (day * 86_400 + seconds - minutes * 60) * 10 + tenths;
                    let value = Temporal::parse(&format!("{date}T{time}{written}")).unwrap();
                    all.push((value, instant));
                }
            }
        }
        for (a, ia) in &all {
            for (b, ib) in &all {
                assert_eq!(a.cmp_chronological(b), Some(ia.cmp(ib)), "{a} {b}");
            }
        }
        assert_eq!(all.len(), 8 * 3 * 5);
    }

    /// Every digit string over `alphabet` of at most `max` digits, the
    /// empty one included.
    fn digit_strings(alphabet: &[char], max: usize) -> Vec<String> {
        let mut all = vec![String::new()];
        let mut layer = vec![String::new()];
        for _ in 0..max {
            layer = layer
                .iter()
                .flat_map(|f| alphabet.iter().map(move |d| format!("{f}{d}")))
                .collect();
            all.extend(layer.iter().cloned());
        }
        all
    }

    /// The fraction `0.digits` as an integer over 10^width.
    fn over_power_of_ten(digits: &str, width: usize) -> u64 {
        format!("{digits:0<width$}").parse().unwrap()
    }

    /// `Fraction::cmp_numeric` against the order of the fractions as
    /// integers over a common power of ten, on every pair of fractions of at
    /// most three digits and on every pair of at most twelve digits over the
    /// digits 0 and 9; on the first scope, also `cmp_chronological` of two
    /// times with those fractions against the former comparison of
    /// nanoseconds, which it must still give on fractions of at most nine
    /// digits.
    #[test]
    fn fractions_order_as_decimal_fractions_on_the_stated_scope() {
        use panproto_expr::exact::Fraction;
        let short = digit_strings(&['0', '1', '2', '3', '4', '5', '6', '7', '8', '9'], 3);
        let long = digit_strings(&['0', '9'], 12);
        let mut pairs = 0_u64;
        for (scope, width) in [(&short, 3), (&long, 12)] {
            let fractions: Vec<(Fraction, u64)> = scope
                .iter()
                .map(|d| (Fraction::new(d).unwrap(), over_power_of_ten(d, width)))
                .collect();
            for (a, va) in &fractions {
                for (b, vb) in &fractions {
                    assert_eq!(a.cmp_numeric(b), va.cmp(vb), "{a:?} {b:?}");
                    pairs += 1;
                }
            }
        }
        // The former comparison: whole seconds, then nanoseconds.
        let time = |d: &str| {
            let dot = if d.is_empty() { "" } else { "." };
            Temporal::parse(&format!("12:00:00{dot}{d}Z")).unwrap()
        };
        let times: Vec<(Temporal, u64)> = short
            .iter()
            .map(|d| (time(d), over_power_of_ten(d, 9)))
            .collect();
        for (ta, na) in &times {
            for (tb, nb) in &times {
                assert_eq!(ta.cmp_chronological(tb), Some(na.cmp(nb)), "{ta} {tb}");
            }
        }
        println!(
            "bound: {} fractions of at most 3 digits and {} of at most 12 digits over 0 and 9; {pairs} pairs",
            short.len(),
            long.len()
        );
        assert_eq!(pairs, 1111 * 1111 + 8191 * 8191);
    }
}
