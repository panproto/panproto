//! Exact values: decimals keep their scale and compare as numbers,
//! integers have one representation, temporals keep the offset and the
//! precision their lexeme wrote, and the expression language compares and
//! converts them without ever rounding one silently.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod exact {
    use std::cmp::Ordering;

    use panproto_expr::exact::{
        BigIntValue, CivilDate, CivilTime, DecimalValue, ExactInteger, Offset, Temporal, WideInt,
        ZuluCase, binary32_bits,
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
                (d.negative, d.digits.as_str(), d.exponent.clone()),
                (negative, digits, WideInt::from(exponent)),
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
        // An exponent of any size is read exactly.
        assert_eq!(dec("1e99999999999").exponent.to_string(), "99999999999");
        assert_eq!(dec("1.50").scale(), WideInt::from(2_i64));
        assert_eq!(dec("1e3").scale(), WideInt::zero());
        assert_eq!(dec("1.50").minimal_scale(), WideInt::from(1_i64));
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
            serde_json::from_str(r#"{"negative":true,"digits":"150","exponent":"-2"}"#).unwrap();
        assert_eq!(ok, dec("-1.50"));
        for bad in [
            r#"{"negative":false,"digits":"015","exponent":"0"}"#,
            r#"{"negative":true,"digits":"0","exponent":"0"}"#,
            r#"{"negative":false,"digits":"15","exponent":"-0"}"#,
            r#"{"negative":false,"digits":"15","exponent":"+1"}"#,
            r#"{"negative":false,"digits":"15","exponent":"01"}"#,
            r#"{"negative":false,"digits":"15","exponent":1}"#,
            r#"{"negative":false,"digits":"15","exponent":"1","scale":1}"#,
        ] {
            assert!(serde_json::from_str::<DecimalValue>(bad).is_err(), "{bad}");
        }
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

    /// Rust's scientific float formatter is an independent shortest-text
    /// oracle. Its only documented difference from this crate's rule is
    /// the choice at an exact tie: Rust takes the larger candidate, while
    /// this crate takes the candidate whose last digit is even.
    #[test]
    fn shortest_float_decimals_match_an_independent_oracle_on_a_bounded_scope() {
        let check64 = |x: f64| {
            let ours = DecimalValue::from_f64_shortest(x).unwrap();
            assert_eq!(ours.to_f64().to_bits(), x.to_bits(), "{x:e}");
            let rust = dec(&format!("{x:e}"));
            assert_eq!(
                ours.significant_digits(),
                rust.significant_digits(),
                "{x:e}: {ours} against {rust}"
            );
            if ours.cmp_numeric(&rust).is_ne() {
                assert_eq!(rust.to_f64().to_bits(), x.to_bits(), "{x:e}: {rust}");
                assert_eq!(
                    (ours.digits.as_bytes().last().unwrap() - b'0') % 2,
                    0,
                    "{x:e}: {ours} against {rust}"
                );
                assert_eq!(
                    (rust.digits.as_bytes().last().unwrap() - b'0') % 2,
                    1,
                    "{x:e}: {ours} against {rust}"
                );
            }
        };
        let check32 = |x: f32| {
            let ours = DecimalValue::from_f32_shortest(x).unwrap();
            assert_eq!(ours.to_f32().to_bits(), x.to_bits(), "{x:e}");
            let rust = dec(&format!("{x:e}"));
            assert_eq!(
                ours.significant_digits(),
                rust.significant_digits(),
                "{x:e}: {ours} against {rust}"
            );
            if ours.cmp_numeric(&rust).is_ne() {
                assert_eq!(rust.to_f32().to_bits(), x.to_bits(), "{x:e}: {rust}");
                assert_eq!(
                    (ours.digits.as_bytes().last().unwrap() - b'0') % 2,
                    0,
                    "{x:e}: {ours} against {rust}"
                );
                assert_eq!(
                    (rust.digits.as_bytes().last().unwrap() - b'0') % 2,
                    1,
                    "{x:e}: {ours} against {rust}"
                );
            }
        };

        let mut state64 = 0x243f_6a88_85a3_08d3_u64;
        for exponent in [0_u64, 1, 2, 255, 511, 1022, 1023, 1024, 1535, 2045, 2046] {
            for _ in 0..128 {
                state64 ^= state64 << 13;
                state64 ^= state64 >> 7;
                state64 ^= state64 << 17;
                let fraction = state64 & ((1_u64 << 52) - 1);
                let x = f64::from_bits((exponent << 52) | fraction);
                if x.is_finite() {
                    check64(x);
                }
            }
        }

        let mut state32 = 0x9e37_79b9_u32;
        for exponent in [0_u32, 1, 2, 31, 63, 126, 127, 128, 191, 253, 254] {
            for _ in 0..128 {
                state32 ^= state32 << 13;
                state32 ^= state32 >> 17;
                state32 ^= state32 << 5;
                let fraction = state32 & ((1_u32 << 23) - 1);
                let x = f32::from_bits((exponent << 23) | fraction);
                if x.is_finite() {
                    check32(x);
                }
            }
        }
    }

    #[test]
    fn decimal_scale_conversion_and_display_boundaries_are_exact() {
        let padded = dec("1.5000");
        assert_eq!(padded.minimal_scale(), WideInt::from(1_i64));
        assert_eq!(padded.significant_digits(), WideInt::from(2_i64));
        assert_eq!(dec("123.4500").significant_digits(), WideInt::from(5_i64));
        assert_eq!(dec("0.00100").significant_digits(), WideInt::from(3_i64));

        assert_eq!(dec("0.1").to_f32().to_bits(), 0.1_f32.to_bits());
        assert_eq!(dec("3.4028235e38").to_f32().to_bits(), f32::MAX.to_bits());
        assert_eq!(dec("1e-45").to_f32().to_bits(), 1);
        assert_eq!(dec("-1e-45").to_f32().to_bits(), 0x8000_0001);

        // Six leading zeroes remain positional; the seventh switches to an
        // exponent. This is the strict boundary in `Display`.
        assert_eq!(dec("1e-7").to_string(), "0.0000001");
        assert_eq!(dec("1e-8").to_string(), "1e-8");
        assert_eq!(dec("12e-8").to_string(), "0.00000012");
        assert_eq!(dec("12e-9").to_string(), "12e-9");
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
            prop_assert_eq!(a.scale(), WideInt::from(frac_part.len()));
            prop_assert_eq!(b.scale(), WideInt::from(padded_frac.len()));
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
        assert_eq!(above.to_string(), "9223372036854775808");
        assert_eq!(
            (above.significand.as_str(), above.zeros.clone()),
            ("9223372036854775808", WideInt::zero())
        );
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
        for bad in [
            r#"{"negative":false,"significand":"42","zeros":"0"}"#,
            r#"{"negative":false,"significand":"09223372036854775808","zeros":"0"}"#,
            r#"{"negative":false,"significand":"10","zeros":"20"}"#,
            r#"{"negative":false,"significand":"1","zeros":"-20"}"#,
        ] {
            assert!(serde_json::from_str::<BigIntValue>(bad).is_err(), "{bad}");
        }
        let big: BigIntValue = serde_json::from_str(
            r#"{"negative":false,"significand":"9223372036854775808","zeros":"0"}"#,
        )
        .unwrap();
        assert_eq!(big, above);
        assert_eq!(
            Literal::integer(BigIntValue::parse("7").unwrap()),
            Literal::Int(7)
        );
    }

    /// The decimal-string implementation of `WideInt` agrees with a
    /// primitive-integer oracle throughout a bounded scope that crosses
    /// zero in both operands. Explicit long carries and borrows cover the
    /// part of the implementation outside that scope.
    #[test]
    fn wide_integer_arithmetic_agrees_with_i128_on_a_bounded_scope() {
        for n in -256_i64..=256 {
            let value = WideInt::from(n);
            assert_eq!(value.to_string(), n.to_string());
            assert_eq!(
                WideInt::parse_canonical(&n.to_string()),
                Some(value.clone())
            );
            assert_eq!(value.to_i64(), Some(n));
            assert_eq!(value.is_zero(), n == 0);
            assert_eq!(value.is_negative(), n < 0);
            assert_eq!(value.abs().to_string(), n.unsigned_abs().to_string());
            assert_eq!(value.negated().to_string(), (-i128::from(n)).to_string());
        }
        for a in -128_i64..=128 {
            for b in -128_i64..=128 {
                let wide_a = WideInt::from(a);
                let wide_b = WideInt::from(b);
                assert_eq!(wide_a.cmp(&wide_b), a.cmp(&b), "{a} cmp {b}");
                assert_eq!(
                    wide_a.plus(&wide_b).to_string(),
                    (i128::from(a) + i128::from(b)).to_string(),
                    "{a} + {b}"
                );
                assert_eq!(
                    wide_a.minus(&wide_b).to_string(),
                    (i128::from(a) - i128::from(b)).to_string(),
                    "{a} - {b}"
                );
            }
        }

        let nines = WideInt::parse_canonical("99999999999999999999").unwrap();
        let power = WideInt::parse_canonical("100000000000000000000").unwrap();
        assert_eq!(nines.plus(&WideInt::from(1_i64)), power);
        assert_eq!(power.minus(&WideInt::from(1_i64)), nines);
        assert_eq!(
            WideInt::parse_canonical("-99999999999999999999")
                .unwrap()
                .minus(&WideInt::from(1_i64)),
            WideInt::parse_canonical("-100000000000000000000").unwrap()
        );
        assert_eq!(
            WideInt::parse_canonical("9223372036854775807")
                .unwrap()
                .to_i64(),
            Some(i64::MAX)
        );
        assert_eq!(
            WideInt::parse_canonical("9223372036854775808")
                .unwrap()
                .to_i64(),
            None
        );
        assert_eq!(
            WideInt::parse_canonical("-9223372036854775808")
                .unwrap()
                .to_i64(),
            Some(i64::MIN)
        );
        assert_eq!(
            WideInt::parse_canonical("-9223372036854775809")
                .unwrap()
                .to_i64(),
            None
        );

        for invalid in ["", "-", "+1", "00", "01", "-0", "-01", "1a"] {
            assert_eq!(WideInt::parse_canonical(invalid), None, "{invalid:?}");
        }
        assert_eq!(
            WideInt::from_sign_and_digits(true, "000"),
            Some(WideInt::zero())
        );
        assert_eq!(WideInt::from_sign_and_digits(false, ""), None);
        assert_eq!(WideInt::from_sign_and_digits(false, "12x"), None);

        for (value, expected) in [
            (WideInt::from(-7_i32), "-7"),
            (WideInt::from(7_u32), "7"),
            (WideInt::from(7_u64), "7"),
            (WideInt::from(7_usize), "7"),
            (WideInt::from(-7_i128), "-7"),
            (WideInt::from(0_i128), "0"),
            (WideInt::from(7_i128), "7"),
        ] {
            assert_eq!(value.to_string(), expected);
        }
        let canonical = WideInt::parse_canonical("-123").unwrap();
        let mut bytes = Vec::new();
        canonical.write_canonical(&mut bytes);
        assert_eq!(bytes, [1, 3, 0, 0, 0, 0, 0, 0, 0, b'1', b'2', b'3']);

        for n in [-1_i128, 0, 1, i128::from(i64::MAX) + 1, i128::MIN] {
            let value = BigIntValue::from_i128(n);
            assert_eq!(value.to_string(), n.to_string(), "{n}");
        }
    }

    #[test]
    fn fidelity_reasons_have_stable_diagnostic_text() {
        let cases = [
            (FidelityReason::Overflow, "out of range"),
            (
                FidelityReason::PrecisionExceeded { max: 17 },
                "not exactly a value of the kind, whose values are written in at most 17 significant digits",
            ),
            (
                FidelityReason::ScaleExceeded { max: 4 },
                "more than 4 digits after the point",
            ),
            (FidelityReason::NotANumber, "not a number"),
            (
                FidelityReason::NonFinite,
                "a NaN or an infinity the kind cannot hold",
            ),
            (FidelityReason::InvalidTemporal, "not a valid date or time"),
            (FidelityReason::OffsetRequired, "a UTC offset is required"),
            (
                FidelityReason::OffsetForbidden,
                "a UTC offset is not allowed",
            ),
            (
                FidelityReason::InvalidEncoding,
                "not in the declared text encoding",
            ),
            (
                FidelityReason::NotANumberForKind,
                "a number where the kind holds none",
            ),
        ];
        for (reason, expected) in cases {
            assert_eq!(reason.to_string(), expected);
        }
    }

    #[test]
    fn temporal_offsets_and_precision_follow_the_lexeme() {
        let offset = |text: &str| Temporal::parse(text).unwrap().offset();
        assert_eq!(
            offset("2026-09-28T12:34:56Z"),
            Some(Offset::Zulu(ZuluCase::Upper))
        );
        assert_eq!(
            offset("2026-09-28T12:34:56z"),
            Some(Offset::Zulu(ZuluCase::Lower))
        );
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
        let Temporal::DateTime(_, time, _, _) = leap else {
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
        // A big integer is a significand and a count of trailing zeros, and
        // its decimal keeps that shape: 10^20 is 1 x 10^20, equal as a
        // number to the integer written out.
        assert_eq!(
            call(BuiltinOp::ParseDecimal, std::slice::from_ref(&big)),
            d("1e20")
        );
        let Literal::Decimal(parsed) = call(BuiltinOp::ParseDecimal, std::slice::from_ref(&big))
        else {
            panic!("a decimal")
        };
        assert!(
            parsed
                .cmp_numeric(&DecimalValue::parse("100000000000000000000").unwrap())
                .is_eq()
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
            r#"{"Decimal":{"negative":false,"digits":"150","exponent":"-2"}}"#
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
    /// little-endian `i64`, month, day), for a date-time the separator byte
    /// (`T`, `t` or a space), the time (hour, minute, second, the
    /// fraction's digit count as a little-endian `u64`, its ASCII digits)
    /// and the offset (`offset`: the tag 0 local, 1 `Z` followed by the
    /// letter's byte, 2 fixed followed by its minutes as a little-endian
    /// `i64`, 3 `-00:00`).
    fn layout(
        date: Option<(i64, u8, u8)>,
        separator: Option<u8>,
        time: Option<(u8, u8, u8, &str)>,
        offset: &[u8],
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
        out.extend(separator);
        if let Some((hour, minute, second, fraction)) = time {
            out.extend_from_slice(&[hour, minute, second]);
            out.extend_from_slice(&u64::try_from(fraction.len()).unwrap().to_le_bytes());
            out.extend_from_slice(fraction.as_bytes());
            out.extend_from_slice(offset);
        }
        out
    }

    /// The offset bytes of a fixed offset of `minutes`.
    fn fixed(minutes: i64) -> Vec<u8> {
        let mut out = vec![2];
        out.extend_from_slice(&minutes.to_le_bytes());
        out
    }

    #[test]
    fn temporal_canonical_bytes_follow_the_layout() {
        let digits = "123456789012345678901234567890";
        let offsets: [(&str, Vec<u8>); 7] = [
            ("", vec![0]),
            ("Z", vec![1, b'Z']),
            ("z", vec![1, b'z']),
            ("+00:00", fixed(0)),
            ("-08:00", fixed(-480)),
            ("+23:59", fixed(1439)),
            ("-00:00", vec![3]),
        ];
        for (written, offset) in &offsets {
            for n in [0, 3, 9, 10, 30] {
                let fraction = &digits[..n];
                let dot = if n == 0 { "" } else { "." };
                let time = format!("07:08:60{dot}{fraction}{written}");
                assert_eq!(
                    Temporal::parse(&time).unwrap().canonical_bytes(),
                    layout(None, None, Some((7, 8, 60, fraction)), offset),
                    "{time}"
                );
                for separator in *b"Tt " {
                    let date_time = format!(
                        "-0044-03-15{}07:08:09{dot}{fraction}{written}",
                        char::from(separator)
                    );
                    assert_eq!(
                        Temporal::parse(&date_time).unwrap().canonical_bytes(),
                        layout(
                            Some((-44, 3, 15)),
                            Some(separator),
                            Some((7, 8, 9, fraction)),
                            offset
                        ),
                        "{date_time}"
                    );
                }
            }
        }
        assert_eq!(
            Temporal::parse("2026-09-28").unwrap().canonical_bytes(),
            layout(Some((2026, 9, 28)), None, None, &[])
        );
        // The written precision reaches the bytes.
        let bytes = |t: &str| Temporal::parse(t).unwrap().canonical_bytes();
        assert_ne!(bytes("12:00:00.1"), bytes("12:00:00.10"));
        assert_ne!(bytes("12:00:00"), bytes("12:00:00.0"));
        // The three zero offsets, and the two cases of `Z`, have distinct
        // encodings, as have the three separators.
        let zeros = ["12:00:00Z", "12:00:00z", "12:00:00+00:00", "12:00:00-00:00"].map(bytes);
        for i in 0..zeros.len() {
            for j in 0..i {
                assert_ne!(zeros[i], zeros[j]);
            }
        }
        let separated = [
            "2024-01-02T03:04:05Z",
            "2024-01-02t03:04:05Z",
            "2024-01-02 03:04:05Z",
        ]
        .map(bytes);
        assert!(separated[0] != separated[1] && separated[1] != separated[2]);
        assert_ne!(separated[0], separated[2]);
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
    /// candidates of equal length to even, and keeps the scale-free form of
    /// the digits.
    #[test]
    fn shortest_text_breaks_a_tie_to_even() {
        // 2229016984128891.25 lies exactly between the 17-digit decimals
        // 2229016984128891.2 and 2229016984128891.3, both of which read back
        // to it; the even one is its shortest text.
        let x = f64::from_bits(0x431f_ad1d_d626_35ed);
        assert_eq!(format!("{x:.2}"), "2229016984128891.25");
        for text in ["2229016984128891.2", "2229016984128891.3"] {
            assert_eq!(text.parse::<f64>().unwrap().to_bits(), x.to_bits());
        }
        assert_eq!(
            DecimalValue::from_f64_shortest(x).unwrap(),
            dec("2229016984128891.2")
        );
        assert_eq!(
            DecimalValue::from_f64_shortest(-x).unwrap(),
            dec("-2229016984128891.2")
        );
        // 2^50 + 1/4 and 2^50 + 3/4: ties at the seventeenth digit, which
        // go to .2 and .8.
        let quarter = f64::from_bits(0x4310_0000_0000_0001);
        let three_quarters = f64::from_bits(0x4310_0000_0000_0003);
        assert_eq!(
            DecimalValue::from_f64_shortest(quarter).unwrap(),
            dec("1125899906842624.2")
        );
        assert_eq!(
            DecimalValue::from_f64_shortest(three_quarters).unwrap(),
            dec("1125899906842624.8")
        );
        // At binary32, 52401.3125 lies between 52401.312 and 52401.313.
        let tie32 = 838_421.0_f32 / 16.0;
        assert_eq!(
            DecimalValue::from_f32_shortest(tie32).unwrap(),
            dec("52401.312")
        );
        assert_eq!(
            DecimalValue::from_f32_shortest(-tie32).unwrap(),
            dec("-52401.312")
        );
        // No tie, no infinity: the shortest texts of a few values.
        assert_eq!(DecimalValue::from_f32_shortest(0.1).unwrap(), dec("0.1"));
        assert_eq!(DecimalValue::from_f32_shortest(f32::NAN), None);
        assert_eq!(DecimalValue::from_f32_shortest(f32::NEG_INFINITY), None);
        assert_eq!(DecimalValue::from_f32_shortest(-0.0).unwrap(), dec("0"));
        assert_eq!(
            DecimalValue::from_f32_shortest(f32::MAX).unwrap(),
            dec("3.4028235e38")
        );
        assert_eq!(
            DecimalValue::from_f32_shortest(f32::from_bits(1)).unwrap(),
            dec("1e-45")
        );
        assert_eq!(
            DecimalValue::from_f64_shortest(5e-324).unwrap(),
            dec("5e-324")
        );
        assert_eq!(
            DecimalValue::from_f64_shortest(f64::MAX).unwrap(),
            dec("1.7976931348623157e308")
        );
        assert_eq!(DecimalValue::from_f64_shortest(100.0).unwrap(), dec("1e2"));
        // A positive power of ten is kept as an exponent.
        let thousand = dec("1e3");
        assert_eq!(
            (
                thousand.digits.as_str(),
                thousand.exponent.clone(),
                thousand.scale()
            ),
            ("1", WideInt::from(3_i64), WideInt::zero())
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

    /// The departures the rustdoc of `Temporal` records hold as stated: a
    /// leap second compares `Equal` to the next minute's second 0 (and, with
    /// a fraction, after it), and neither a plus sign before a year nor a
    /// minus sign before the year zero is written back.
    #[test]
    fn the_recorded_temporal_departures_hold_as_stated() {
        let parse = |text: &str| Temporal::parse(text).unwrap();
        let leap = parse("2024-01-01T23:59:60Z");
        let midnight = parse("2024-01-02T00:00:00Z");
        assert_eq!(leap.cmp_chronological(&midnight), Some(Ordering::Equal));
        assert_ne!(leap, midnight);
        assert_eq!(
            parse("2024-01-01T23:59:60.5Z").cmp_chronological(&midnight),
            Some(Ordering::Greater)
        );
        assert_eq!(
            parse("2024-01-01T23:59:59.999Z").cmp_chronological(&leap),
            Some(Ordering::Less)
        );
        assert_eq!(parse("+2024-01-01").to_string(), "2024-01-01");
        assert_eq!(parse("+2024-01-01"), parse("2024-01-01"));
        // A minus sign before the year zero is read as the year zero and
        // not written back.
        assert_eq!(parse("-0000-01-01").to_string(), "0000-01-01");
        assert_eq!(parse("-0000-01-01"), parse("0000-01-01"));
        assert_eq!(
            parse("-0000-01-01T00:00:00Z").to_string(),
            "0000-01-01T00:00:00Z"
        );
        // A minus sign before any other year is kept.
        assert_eq!(parse("-0001-01-01").to_string(), "-0001-01-01");
        assert_eq!(
            parse("+2024-01-01T00:00:00Z").to_string(),
            "2024-01-01T00:00:00Z"
        );
    }

    /// The canonical NaN is one constant defined by its bits.
    #[test]
    fn the_canonical_nan_is_defined_by_its_bits() {
        use panproto_expr::exact::{CANONICAL_NAN, NEGATIVE_CANONICAL_NAN};
        assert_eq!(CANONICAL_NAN.to_bits(), 0x7ff8_0000_0000_0000);
        assert_eq!(
            panproto_expr::CANONICAL_NAN.to_bits(),
            0x7ff8_0000_0000_0000
        );
        assert!(CANONICAL_NAN.is_nan());
        assert_eq!(NEGATIVE_CANONICAL_NAN.to_bits(), 0xfff8_0000_0000_0000);
        assert_eq!(
            panproto_expr::NEGATIVE_CANONICAL_NAN.to_bits(),
            0xfff8_0000_0000_0000
        );
        assert!(NEGATIVE_CANONICAL_NAN.is_nan());
        assert!(NEGATIVE_CANONICAL_NAN.is_sign_negative());
    }

    /// `binary32_bits` decides on the bits: a NaN is held exactly when its
    /// low 29 fraction bits are zero, quiet or signalling.
    #[test]
    fn binary32_bits_narrows_on_the_bits() {
        let bits = |b: u64| binary32_bits(f64::from_bits(b));
        assert_eq!(bits(0x7ff0_0000_2000_0000), Some(0x7f80_0001));
        assert_eq!(bits(0x7ff8_0000_0000_0000), Some(0x7fc0_0000));
        assert_eq!(bits(0xfff8_0000_0000_0000), Some(0xffc0_0000));
        assert_eq!(bits(0x7ff8_0000_2000_0000), Some(0x7fc0_0001));
        assert_eq!(bits(0x7ff8_0000_0000_1234), None);
        assert_eq!(bits(0x7ff0_0000_0000_0001), None);
        assert_eq!(bits(0xfff8_0000_1fff_ffff), None);
        assert_eq!(binary32_bits(0.1), None);
        assert_eq!(binary32_bits(0.100_000_001_490_116_12), Some(0x3dcc_cccd));
        assert_eq!(binary32_bits(f64::INFINITY), Some(0x7f80_0000));
        assert_eq!(binary32_bits(f64::NEG_INFINITY), Some(0xff80_0000));
        assert_eq!(binary32_bits(0.0), Some(0));
        assert_eq!(binary32_bits(-0.0), Some(0x8000_0000));
        assert_eq!(binary32_bits(f64::from(f32::MAX)), Some(f32::MAX.to_bits()));
        assert_eq!(
            binary32_bits(f64::from(f32::MIN_POSITIVE)),
            Some(0x0080_0000)
        );
        assert_eq!(binary32_bits(f64::from(f32::from_bits(1))), Some(1));
        assert_eq!(
            binary32_bits(f64::from(f32::from_bits(0x007f_ffff))),
            Some(0x007f_ffff)
        );
        assert_eq!(
            binary32_bits(f64::from(f32::from_bits(0x8040_0001))),
            Some(0x8040_0001)
        );
        // Half the least binary32 subnormal, a binary64 normal value, and the
        // value just above the largest binary32 are not binary32 values.
        assert_eq!(binary32_bits(2.0_f64.powi(-150)), None);
        assert_eq!(binary32_bits(f64::from(f32::MAX) * 2.0), None);
        assert_eq!(binary32_bits(f64::from_bits(1)), None);
        assert_eq!(binary32_bits(1.0 + 2.0_f64.powi(-24)), None);
        assert_eq!(binary32_bits(1.0 + 2.0_f64.powi(-23)), Some(0x3f80_0001));
    }

    /// A `BigIntValue` is a significand and a count of trailing zeros, and
    /// its canonical bytes are the sign byte, the significand behind its
    /// length and the count's decimal digits behind theirs.
    #[test]
    fn bigint_canonical_bytes_follow_the_layout() {
        fn layout(negative: bool, significand: &str, zeros: &str) -> Vec<u8> {
            let mut out = vec![u8::from(negative)];
            for part in [significand, zeros] {
                out.extend_from_slice(&u64::try_from(part.len()).unwrap().to_le_bytes());
                out.extend_from_slice(part.as_bytes());
            }
            out
        }
        let big = |text: &str| match BigIntValue::parse(text).unwrap() {
            ExactInteger::Big(b) => b,
            ExactInteger::Int(n) => panic!("{text} is the i64 {n}"),
        };
        let from_exponent =
            |negative: bool, significand: &str, zeros: &str| match BigIntValue::from_scaled(
                negative,
                significand,
                &WideInt::parse_canonical(zeros).unwrap(),
            )
            .unwrap()
            {
                ExactInteger::Big(b) => b,
                ExactInteger::Int(n) => panic!("the i64 {n}"),
            };
        for (negative, significand, zeros) in [
            (false, "9223372036854775808", "0"),
            (true, "9223372036854775809", "0"),
            (false, "1", "19"),
            (false, "1", "4097"),
            (true, "12", "4097"),
            (false, "7", "100000000000000000000"),
        ] {
            let value = from_exponent(negative, significand, zeros);
            assert_eq!(
                value.canonical_bytes(),
                layout(negative, significand, zeros)
            );
            assert!(value.is_well_formed());
            if zeros.len() < 5 {
                // The integer written out reads as the same value.
                let sign = if negative { "-" } else { "" };
                let count = zeros.parse::<usize>().unwrap();
                let written = format!("{sign}{significand}{}", "0".repeat(count));
                assert_eq!(big(&written), value, "{written}");
                assert_eq!(value.to_string(), written);
            }
        }
        // 10^4097: the 22 bytes 00, 01 00 00 00 00 00 00 00, 31,
        // 04 00 00 00 00 00 00 00, 34 30 39 37.
        assert_eq!(
            from_exponent(false, "1", "4097").canonical_bytes(),
            [
                0x00, 0x01, 0, 0, 0, 0, 0, 0, 0, 0x31, 0x04, 0, 0, 0, 0, 0, 0, 0, 0x34, 0x30, 0x39,
                0x37
            ]
        );
        // Trailing zeros of the written significand move into the count.
        assert_eq!(
            from_exponent(false, "1000", "17"),
            from_exponent(false, "1", "20")
        );
        // Values that fit i64 are never big, whatever their spelling.
        assert_eq!(
            BigIntValue::from_scaled(false, "1", &WideInt::from(18_i64)),
            Some(ExactInteger::Int(1_000_000_000_000_000_000))
        );
        // Two significant digits and ten appended zeros still fit i64.
        // The boundary depends on addition, not multiplication, of those
        // two counts.
        assert_eq!(
            BigIntValue::from_scaled(false, "12", &WideInt::from(10_i64)),
            Some(ExactInteger::Int(120_000_000_000))
        );
        assert_eq!(
            BigIntValue::from_scaled(true, "9223372036854775808", &WideInt::zero()),
            Some(ExactInteger::Int(i64::MIN))
        );
        assert_eq!(
            BigIntValue::from_scaled(false, "15", &WideInt::from(-1_i64)),
            None
        );
        assert_eq!(
            BigIntValue::from_scaled(false, "150", &WideInt::from(-1_i64)),
            Some(ExactInteger::Int(15))
        );
        assert_eq!(BigIntValue::from_scaled(false, "", &WideInt::zero()), None);
        // Numeric order follows the written-out integers.
        let order = [
            from_exponent(true, "1", "30"),
            from_exponent(true, "99", "20"),
            from_exponent(true, "9223372036854775809", "0"),
            from_exponent(false, "9223372036854775808", "0"),
            from_exponent(false, "1", "19"),
            from_exponent(false, "11", "18"),
            from_exponent(false, "2", "19"),
            from_exponent(false, "1", "4097"),
        ];
        for (i, a) in order.iter().enumerate() {
            for (j, b) in order.iter().enumerate() {
                assert_eq!(a.cmp_numeric(b), i.cmp(&j), "{a:?} {b:?}");
            }
        }
    }

    /// Formatting a finite-size big integer terminates and writes exactly
    /// its counted zeroes. The channel bound turns a regressed subtraction
    /// loop into an assertion failure rather than letting the test process
    /// inherit the loop.
    #[test]
    fn bigint_display_terminates_at_the_zero_count() {
        use std::sync::mpsc;
        use std::time::Duration;

        let Some(ExactInteger::Big(value)) =
            BigIntValue::from_scaled(false, "123", &WideInt::from(20_i64))
        else {
            panic!("123e20 is outside i64")
        };
        let (send, receive) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = send.send(value.to_string());
        });
        let text = receive
            .recv_timeout(Duration::from_secs(2))
            .expect("formatting a 23-digit bigint must terminate");
        assert_eq!(text, "12300000000000000000000");
    }

    /// The serde form carries the count, and the former `{negative,
    /// digits}` shape is refused.
    #[test]
    fn bigint_serde_refuses_the_former_shape() {
        let value: BigIntValue =
            serde_json::from_str(r#"{"negative":true,"significand":"12","zeros":"4097"}"#).unwrap();
        assert_eq!(
            serde_json::to_string(&value).unwrap(),
            r#"{"negative":true,"significand":"12","zeros":"4097"}"#
        );
        assert_eq!(value.exponent_form(), "-12e4097");
        for former in [
            r#"{"negative":false,"digits":"9223372036854775808"}"#,
            r#"{"negative":false,"digits":"9223372036854775808","significand":"9223372036854775808","zeros":"0"}"#,
        ] {
            assert!(
                serde_json::from_str::<BigIntValue>(former).is_err(),
                "{former}"
            );
        }
    }

    /// A count of trailing zeros of any size is held in the space of the
    /// lexeme that wrote it, compared and converted without writing a digit
    /// beyond the significand.
    #[test]
    fn a_bigint_with_a_huge_count_is_held_in_the_space_of_its_lexeme() {
        let count = WideInt::parse_canonical("100000000000000000000").unwrap();
        let Some(ExactInteger::Big(huge)) = BigIntValue::from_scaled(false, "1", &count) else {
            panic!("10^(10^20) is a big integer")
        };
        assert_eq!(huge.exponent_form(), "1e100000000000000000000");
        assert_eq!(huge.canonical_bytes().len(), 1 + 8 + 1 + 8 + 21);
        assert_eq!(huge.digit_count().to_string(), "100000000000000000001");
        assert_eq!(huge.to_f64().to_bits(), f64::INFINITY.to_bits());
        let Some(ExactInteger::Big(negative)) = BigIntValue::from_scaled(true, "1", &count) else {
            panic!("-10^(10^20) is a big integer")
        };
        assert_eq!(negative.to_f64().to_bits(), f64::NEG_INFINITY.to_bits());
        assert_eq!(negative.cmp_numeric(&huge), Ordering::Less);
        let next = WideInt::parse_canonical("100000000000000000001").unwrap();
        let Some(ExactInteger::Big(bigger)) = BigIntValue::from_scaled(false, "1", &next) else {
            panic!("a big integer")
        };
        assert_eq!(huge.cmp_numeric(&bigger), Ordering::Less);
        let json = serde_json::to_string(&huge).unwrap();
        assert!(json.len() < 80, "{json}");
        assert_eq!(serde_json::from_str::<BigIntValue>(&json).unwrap(), huge);
        // As a decimal, digits = significand and exponent = count.
        let decimal = DecimalValue::parse(&huge.exponent_form()).unwrap();
        assert_eq!((decimal.digits.as_str(), decimal.exponent), ("1", count));
        // A moderate count converts to the nearest float.
        let Some(ExactInteger::Big(e300)) =
            BigIntValue::from_scaled(false, "3", &WideInt::from(300_i64))
        else {
            panic!("3e300 is a big integer")
        };
        assert_eq!(e300.to_f64().to_bits(), 3e300_f64.to_bits());
    }

    /// `DecimalValue` holds any exponent; its canonical bytes write the
    /// exponent as a sign byte and the digits of its magnitude behind their
    /// length.
    #[test]
    fn decimal_canonical_bytes_hold_any_exponent() {
        fn layout(
            negative: bool,
            digits: &str,
            exponent_negative: bool,
            exponent: &str,
        ) -> Vec<u8> {
            let mut out = vec![u8::from(negative)];
            out.extend_from_slice(&u64::try_from(digits.len()).unwrap().to_le_bytes());
            out.extend_from_slice(digits.as_bytes());
            out.push(u8::from(exponent_negative));
            out.extend_from_slice(&u64::try_from(exponent.len()).unwrap().to_le_bytes());
            out.extend_from_slice(exponent.as_bytes());
            out
        }
        // "1e3000000000": digits "1", exponent 3000000000.
        assert_eq!(
            dec("1e3000000000").canonical_bytes(),
            layout(false, "1", false, "3000000000")
        );
        assert_eq!(
            dec("1e3000000000").canonical_bytes(),
            [
                [0x00].as_slice(),
                &1_u64.to_le_bytes(),
                b"1",
                &[0x00],
                &10_u64.to_le_bytes(),
                b"3000000000"
            ]
            .concat()
        );
        for (text, negative, digits, exponent_negative, exponent) in [
            ("0.0e-3000000000", false, "0", true, "3000000001"),
            ("-0e3000000000", false, "0", false, "3000000000"),
            ("-1.50", true, "150", true, "2"),
            ("0", false, "0", false, "0"),
            ("-1e-2147483649", true, "1", true, "2147483649"),
            ("1e2147483648", false, "1", false, "2147483648"),
            (
                "1e100000000000000000000",
                false,
                "1",
                false,
                "100000000000000000000",
            ),
            (
                "-1e-100000000000000000000",
                true,
                "1",
                true,
                "100000000000000000000",
            ),
        ] {
            let value = dec(text);
            assert_eq!(
                value.canonical_bytes(),
                layout(negative, digits, exponent_negative, exponent),
                "{text}"
            );
            let json = serde_json::to_string(&value).unwrap();
            assert_eq!(serde_json::from_str::<DecimalValue>(&json).unwrap(), value);
            assert_eq!(dec(&value.to_string()), value, "{text}");
        }
        // Scale, minimal scale and integer digits take the wide exponent.
        let far = dec("1.50e-3000000000");
        assert_eq!(far.scale().to_string(), "3000000002");
        assert_eq!(far.minimal_scale().to_string(), "3000000001");
        assert_eq!(far.integer_digits(), WideInt::zero());
        let big = dec("15e3000000000");
        assert_eq!(big.integer_digits().to_string(), "3000000002");
        assert_eq!(big.scale(), WideInt::zero());
        assert_eq!(big.to_f64().to_bits(), f64::INFINITY.to_bits());
        assert_eq!(
            dec("-1e-3000000000").to_f64().to_bits(),
            (-0.0_f64).to_bits()
        );
        assert_eq!(far.cmp_numeric(&dec("0")), Ordering::Greater);
        assert_eq!(far.cmp_numeric(&dec("1e-3000000000")), Ordering::Greater);
        assert_eq!(dec("1.6e-3000000000").cmp_numeric(&far), Ordering::Greater);
        assert_eq!(
            dec("15e3000000000").cmp_numeric(&dec("1.5e3000000001")),
            Ordering::Equal
        );
    }
}
