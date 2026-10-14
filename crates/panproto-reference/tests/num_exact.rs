#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Exact arithmetic against machine integers and a test-local decimal
//! construction.
//!
//! Every expected value is computed here, independently of the module under
//! test: by `u128` and `i128` arithmetic for operands that fit, and by a
//! base-ten digit vector written below for the larger powers and products.
//! The operand sets are exhaustive over small ranges and cover every limb
//! boundary of the base-`10^9` representation.

mod bounded;

use panproto_reference::num::{Int, Nat, Rat, floor_log2, floor_log10};

const BASE: u128 = 1_000_000_000;

/// A deterministic stream of 64-bit values (Knuth's MMIX generator).
struct Lcg(u64);

impl Lcg {
    const fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0
    }

    /// A value of `bits` significant bits at most.
    fn below_bits(&mut self, bits: u32) -> u128 {
        let wide = (u128::from(self.next()) << 64) | u128::from(self.next());
        if bits >= 128 {
            wide
        } else {
            wide & ((1u128 << bits) - 1)
        }
    }
}

/// Values on and around every limb boundary, powers of two and ten, and the
/// extremes of the machine types.
fn boundary_values() -> Vec<u128> {
    let mut out: Vec<u128> = (0..=64).collect();
    for k in 1..=4u32 {
        let b = BASE.pow(k);
        out.extend([b - 1, b, b + 1, 2 * b - 1, 2 * b]);
        out.extend(b.checked_mul(999_999_999));
    }
    for k in 0..=38u32 {
        let p = 10u128.pow(k);
        out.extend([p - 1, p, p + 1]);
        out.extend(p.checked_mul(9));
    }
    for k in 0..=127u32 {
        let p = 1u128 << k;
        out.extend([p - 1, p, p | 1]);
    }
    out.extend([u128::MAX - 1, u128::MAX, u128::MAX / 3, u128::MAX / 7]);
    out.sort_unstable();
    out.dedup();
    out
}

fn nat(v: u128) -> Nat {
    Nat::from_u128(v)
}

const fn gcd_u128(mut a: u128, mut b: u128) -> u128 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

#[test]
fn naturals_read_write_and_measure_like_machine_integers() {
    bounded::within(60, || {
        for v in boundary_values() {
            let n = nat(v);
            let text = v.to_string();
            assert_eq!(n.to_string(), text);
            assert_eq!(format!("{n:?}"), text);
            assert_eq!(n.to_u128(), Some(v));
            assert_eq!(n.to_u64(), u64::try_from(v).ok(), "to_u64 of {v}");
            assert_eq!(Nat::parse(&text), Some(n.clone()));
            assert_eq!(Nat::parse(&format!("0000000000{text}")), Some(n.clone()));
            assert_eq!(n.is_zero(), v == 0);
            assert_eq!(n.is_even(), v % 2 == 0, "parity of {v}");
            let digits = if v == 0 { 0 } else { text.len() };
            assert_eq!(n.digit_count(), digits, "digit count of {v}");
            for i in 0..=45usize {
                let expected = u32::try_from(i)
                    .ok()
                    .and_then(|i| 10u128.checked_pow(i))
                    .map_or(0, |p| (v / p) % 10);
                assert_eq!(u128::from(n.digit(i)), expected, "digit {i} of {v}");
            }
            if let Ok(small) = u64::try_from(v) {
                assert_eq!(Nat::from_u64(small), n);
            }
        }
        assert_eq!(Nat::zero().to_string(), "0");
        assert_eq!(Nat::one().to_string(), "1");
        assert_eq!(Nat::default(), Nat::zero());
        assert_eq!(Nat::parse("0"), Some(Nat::zero()));
        assert_eq!(Nat::parse("000"), Some(Nat::zero()));
        assert_eq!(Nat::parse("1000000000"), Some(nat(BASE)));
        assert_eq!(
            Nat::parse("340282366920938463463374607431768211456")
                .unwrap()
                .to_u128(),
            None
        );
        for bad in [
            "", "-1", "+1", " 1", "1 ", "1a", "a1", "0x10", "1_000", "\u{661}", "1.0",
        ] {
            assert_eq!(Nat::parse(bad), None, "parse {bad:?}");
        }
    });
}

fn pairs() -> Vec<(u128, u128)> {
    let mut out = Vec::new();
    for a in 0..=40u128 {
        for b in 0..=40u128 {
            out.push((a, b));
        }
    }
    let boundary = boundary_values();
    let sparse: Vec<u128> = boundary.iter().copied().step_by(3).collect();
    for &a in &sparse {
        for &b in &sparse {
            out.push((a, b));
        }
    }
    let mut lcg = Lcg(0x5EED);
    for _ in 0..20_000 {
        let a_bits = u32::try_from(lcg.next() % 129).unwrap();
        let b_bits = u32::try_from(lcg.next() % 129).unwrap();
        out.push((lcg.below_bits(a_bits), lcg.below_bits(b_bits)));
    }
    out
}

#[test]
fn natural_operations_agree_with_u128_on_every_tested_pair() {
    bounded::within(60, || {
        for (a, b) in pairs() {
            let (x, y) = (nat(a), nat(b));
            assert_eq!(x.cmp(&y), a.cmp(&b), "cmp {a} {b}");
            assert_eq!(x.partial_cmp(&y), Some(a.cmp(&b)), "partial_cmp {a} {b}");
            assert_eq!(x == y, a == b);
            if let Some(sum) = a.checked_add(b) {
                assert_eq!(x.add(&y).to_u128(), Some(sum), "{a} + {b}");
            }
            assert_eq!(
                x.checked_sub(&y).map(|d| d.to_u128()),
                a.checked_sub(b).map(Some),
                "{a} - {b}"
            );
            if let Some(product) = a.checked_mul(b) {
                assert_eq!(x.mul(&y).to_u128(), Some(product), "{a} * {b}");
            }
            let quotient = x.div_rem(&y).map(|(q, r)| (q.to_u128(), r.to_u128()));
            let expected = a.checked_div(b).map(|q| (Some(q), Some(a % b)));
            assert_eq!(quotient, expected, "{a} divmod {b}");
            assert_eq!(
                Nat::gcd(&x, &y).to_u128(),
                Some(gcd_u128(a, b)),
                "gcd {a} {b}"
            );
        }
    });
}

#[test]
fn powers_and_decimal_shifts_agree_with_u128() {
    bounded::within(60, || {
        for base in 0..=12u128 {
            for exp in 0..=40u32 {
                if let Some(p) = base.checked_pow(exp) {
                    assert_eq!(Nat::pow(&nat(base), exp).to_u128(), Some(p), "{base}^{exp}");
                }
            }
        }
        for k in 0..=127u32 {
            assert_eq!(Nat::pow2(k).to_u128(), Some(1u128 << k), "2^{k}");
        }
        for k in 0..=38usize {
            let p = 10u128.pow(u32::try_from(k).unwrap());
            assert_eq!(Nat::pow10(k).to_u128(), Some(p), "10^{k}");
            for v in [0u128, 1, 7, 999_999_999, 123_456_789_012] {
                if let Some(shifted) = v.checked_mul(p) {
                    assert_eq!(nat(v).shift10(k).to_u128(), Some(shifted), "{v} * 10^{k}");
                }
            }
        }
    });
}

/// A natural number as base-ten digits, least significant first, with no
/// leading zero: the test-local construction for values beyond `u128`.
#[derive(Clone, PartialEq, Eq)]
struct Decimal(Vec<u8>);

impl Decimal {
    fn small(mut v: u128) -> Self {
        let mut digits = Vec::new();
        while v > 0 {
            digits.push(u8::try_from(v % 10).unwrap());
            v /= 10;
        }
        Self(digits)
    }

    fn trimmed(mut digits: Vec<u8>) -> Self {
        while digits.last() == Some(&0) {
            digits.pop();
        }
        Self(digits)
    }

    fn add(&self, other: &Self) -> Self {
        let mut out = Vec::new();
        let mut carry = 0u8;
        for i in 0..self.0.len().max(other.0.len()) {
            let s = self.0.get(i).unwrap_or(&0) + other.0.get(i).unwrap_or(&0) + carry;
            out.push(s % 10);
            carry = s / 10;
        }
        out.push(carry);
        Self::trimmed(out)
    }

    fn mul(&self, other: &Self) -> Self {
        let mut out = vec![0u32; self.0.len() + other.0.len() + 1];
        for (i, a) in self.0.iter().enumerate() {
            for (j, b) in other.0.iter().enumerate() {
                out[i + j] += u32::from(*a) * u32::from(*b);
            }
        }
        let mut digits = Vec::with_capacity(out.len());
        let mut carry = 0u32;
        for cell in out {
            let s = cell + carry;
            digits.push(u8::try_from(s % 10).unwrap());
            carry = s / 10;
        }
        assert_eq!(carry, 0);
        Self::trimmed(digits)
    }

    fn pow(base: u128, exp: u32) -> Self {
        let b = Self::small(base);
        (0..exp).fold(Self::small(1), |acc, _| acc.mul(&b))
    }

    fn text(&self) -> String {
        if self.0.is_empty() {
            return "0".to_owned();
        }
        self.0.iter().rev().map(|d| char::from(b'0' + d)).collect()
    }
}

#[test]
fn large_powers_and_products_agree_with_decimal_digit_vectors() {
    bounded::within(60, || {
        let mut two = Decimal::small(1);
        for k in 0..=1_100u32 {
            assert_eq!(Nat::pow2(k).to_string(), two.text(), "2^{k}");
            two = two.add(&two);
        }
        for k in [0u32, 1, 9, 10, 17, 18, 19, 40, 100, 333] {
            let shift = usize::try_from(k).unwrap();
            assert_eq!(
                Nat::pow10(shift).to_string(),
                Decimal::pow(10, k).text(),
                "10^{k}"
            );
            assert_eq!(
                Nat::pow(&nat(7), k).to_string(),
                Decimal::pow(7, k).text(),
                "7^{k}"
            );
        }
        let mut lcg = Lcg(0xD1CE);
        for _ in 0..300 {
            let (left, middle, right) = (
                lcg.below_bits(128),
                lcg.below_bits(100),
                lcg.below_bits(128),
            );
            let product = Decimal::small(left)
                .mul(&Decimal::small(middle))
                .mul(&Decimal::small(right));
            let got = nat(left).mul(&nat(middle)).mul(&nat(right));
            assert_eq!(
                got.to_string(),
                product.text(),
                "{left} * {middle} * {right}"
            );
            let sum = product.add(&Decimal::small(left));
            let total = Nat::parse(&sum.text()).unwrap();
            assert_eq!(total.add(&Nat::zero()).to_string(), sum.text());
            assert_eq!(
                total.checked_sub(&nat(left)).map(|rest| rest.to_string()),
                Some(product.text()),
                "({left} * {middle} * {right} + {left}) - {left}"
            );
            // Long division by a multi-limb divisor, checked by the identity
            // total = quotient * divisor + remainder with remainder < divisor,
            // on decimal digits.
            let divisor = middle.max(1);
            let (quotient, remainder) = total.div_rem(&nat(divisor)).unwrap();
            let rebuilt = Decimal::small(divisor)
                .mul(&decimal_of(&quotient))
                .add(&decimal_of(&remainder));
            assert_eq!(
                rebuilt.text(),
                sum.text(),
                "{} divmod {divisor}",
                sum.text()
            );
            assert!(
                remainder.to_u128().is_some_and(|rest| rest < divisor),
                "remainder below the divisor"
            );
        }
    });
}

fn decimal_of(n: &Nat) -> Decimal {
    let text = n.to_string();
    Decimal::trimmed(text.bytes().rev().map(|b| b - b'0').collect())
}

fn int(v: i128) -> Int {
    Int::from_i128(v)
}

fn int_values() -> Vec<i128> {
    let mut out: Vec<i128> = (-40..=40).collect();
    for p in [
        i128::from(i64::MAX),
        1i128 << 63,
        (1i128 << 63) + 1,
        999_999_999,
        1_000_000_000,
        1_000_000_001,
        1_000_000_000_000_000_000,
        i128::MAX / 3,
    ] {
        out.extend([p, -p]);
    }
    out.extend([i128::MAX, i128::MIN + 1, i128::MIN]);
    out
}

#[test]
fn integers_agree_with_i128() {
    bounded::within(60, || {
        let values = int_values();
        for &a in &values {
            let x = int(a);
            assert_eq!(x.to_string(), a.to_string());
            assert_eq!(format!("{x:?}"), a.to_string());
            assert_eq!(x.is_negative(), a < 0, "sign of {a}");
            assert_eq!(x.is_zero(), a == 0);
            assert_eq!(x.magnitude().to_u128(), Some(a.unsigned_abs()));
            assert_eq!(x.to_i64(), i64::try_from(a).ok(), "to_i64 of {a}");
            assert_eq!(Int::parse(&a.to_string()), Some(x.clone()));
            if a >= 0 {
                assert_eq!(Int::parse(&format!("+{a}")), Some(x.clone()));
            }
            if let Ok(small) = i64::try_from(a) {
                assert_eq!(Int::from_i64(small), x);
            }
            if let Some(negated) = a.checked_neg() {
                assert_eq!(x.neg(), int(negated));
            }
            for &b in &values {
                let y = int(b);
                assert_eq!(x.cmp(&y), a.cmp(&b), "cmp {a} {b}");
                assert_eq!(x.partial_cmp(&y), Some(a.cmp(&b)), "partial_cmp {a} {b}");
                if let Some(sum) = a.checked_add(b) {
                    assert_eq!(x.add(&y), int(sum), "{a} + {b}");
                }
                if let Some(difference) = a.checked_sub(b) {
                    assert_eq!(x.sub(&y), int(difference), "{a} - {b}");
                }
                if let Some(product) = a.checked_mul(b) {
                    assert_eq!(x.mul(&y), int(product), "{a} * {b}");
                }
            }
        }
        assert_eq!(Int::zero(), int(0));
        assert_eq!(Int::default(), int(0));
        assert_eq!(Int::new(true, Nat::zero()).to_string(), "0");
        assert!(!Int::new(true, Nat::zero()).is_negative());
        assert_eq!(Int::new(true, nat(5)).to_string(), "-5");
        assert_eq!(Int::new(false, nat(5)).to_string(), "5");
        assert_eq!(Int::parse("-0"), Some(Int::zero()));
        assert_eq!(Int::parse("+0"), Some(Int::zero()));
        assert_eq!(Int::parse("-007"), Some(int(-7)));
        for bad in ["", "-", "+", "--1", "+-1", "-+1", " -1", "1-", "a"] {
            assert_eq!(Int::parse(bad), None, "parse {bad:?}");
        }
        assert_eq!(Int::from_i64(i64::MIN).to_string(), i64::MIN.to_string());
        assert_eq!(Int::from_i64(i64::MIN).to_i64(), Some(i64::MIN));
    });
}

/// A reduced fraction `n / d` with `d > 0`, computed here.
fn reduced(n: i128, d: i128) -> (i128, i128) {
    assert!(d != 0);
    let g = i128::try_from(gcd_u128(n.unsigned_abs(), d.unsigned_abs())).unwrap();
    let sign = if d < 0 { -1 } else { 1 };
    (sign * n / g, sign * d / g)
}

fn rat(n: i128, d: i128) -> Rat {
    let (n, d) = reduced(n, d);
    Rat::new(int(n), nat(d.unsigned_abs())).unwrap()
}

fn parts(q: &Rat) -> (i128, i128) {
    let n = q.num().to_string().parse::<i128>().unwrap();
    let d = q.den().to_string().parse::<i128>().unwrap();
    (n, d)
}

#[test]
fn rationals_reduce_and_compute_like_i128_fractions() {
    bounded::within(60, || {
        for n in -30..=30i128 {
            assert_eq!(Rat::new(int(n), Nat::zero()), None, "{n}/0");
            for d in 1..=30i128 {
                let q = Rat::new(int(n), nat(d.unsigned_abs())).unwrap();
                assert_eq!(parts(&q), reduced(n, d), "{n}/{d}");
                assert_eq!(q.is_integer(), n % d == 0, "{n}/{d} integral");
                assert_eq!(q.is_zero(), n == 0);
                assert_eq!(q.is_negative(), n < 0);
                assert_eq!(parts(&q.abs()), reduced(n.abs(), d));
                assert_eq!(parts(&q.neg()), reduced(-n, d));
                assert_eq!(q.floor(), int(n.div_euclid(d)), "floor {n}/{d}");
                assert_eq!(format!("{q:?}"), {
                    let (rn, rd) = reduced(n, d);
                    format!("{rn}/{rd}")
                });
            }
        }
        let mut operands = Vec::new();
        for n in -7..=7i128 {
            for d in 1..=7i128 {
                if gcd_u128(n.unsigned_abs(), d.unsigned_abs()) == 1 {
                    operands.push((n, d));
                }
            }
        }
        for &(an, ad) in &operands {
            for &(bn, bd) in &operands {
                let (a, b) = (rat(an, ad), rat(bn, bd));
                assert_eq!(parts(&a.add(&b)), reduced(an * bd + bn * ad, ad * bd));
                assert_eq!(parts(&a.sub(&b)), reduced(an * bd - bn * ad, ad * bd));
                assert_eq!(parts(&a.mul(&b)), reduced(an * bn, ad * bd));
                assert_eq!(
                    a.div(&b).map(|q| parts(&q)),
                    (bn != 0).then(|| reduced(an * bd, ad * bn)),
                    "{an}/{ad} / {bn}/{bd}"
                );
                assert_eq!(
                    a.cmp(&b),
                    (an * bd).cmp(&(bn * ad)),
                    "cmp {an}/{ad} {bn}/{bd}"
                );
                assert_eq!(a.partial_cmp(&b), Some((an * bd).cmp(&(bn * ad))));
            }
        }
        assert_eq!(parts(&Rat::zero()), (0, 1));
        assert_eq!(parts(&Rat::from_int(int(-4))), (-4, 1));
        assert!(Rat::zero().is_zero());
        assert!(Rat::from_int(int(3)).is_integer());
    });
}

#[test]
fn rational_powers_of_two_and_ten_are_exact() {
    bounded::within(60, || {
        for k in -300..=300i64 {
            let e = u32::try_from(k.unsigned_abs()).unwrap();
            let (two, ten) = (Decimal::pow(2, e).text(), Decimal::pow(10, e).text());
            let (p2, p10) = (Rat::pow2(k), Rat::pow10(k));
            if k >= 0 {
                assert_eq!(
                    (p2.num().to_string(), p2.den().to_string()),
                    (two, "1".to_owned())
                );
                assert_eq!(
                    (p10.num().to_string(), p10.den().to_string()),
                    (ten, "1".to_owned())
                );
            } else {
                assert_eq!(
                    (p2.num().to_string(), p2.den().to_string()),
                    ("1".to_owned(), two)
                );
                assert_eq!(
                    (p10.num().to_string(), p10.den().to_string()),
                    ("1".to_owned(), ten)
                );
            }
            assert_eq!(p2.is_integer(), k >= 0, "integrality of 2^{k}");
            assert_eq!(p10.is_integer(), k >= 0, "integrality of 10^{k}");
        }
    });
}

/// `floor(log_b(n / d))` for positive `n` and `d`, by integer search.
const fn floor_log(base: u128, n: u128, d: u128) -> i64 {
    if n >= d {
        let mut e = 0;
        let mut p = d;
        while p * base <= n {
            p *= base;
            e += 1;
        }
        e
    } else {
        let mut k = 0;
        let mut p = n;
        while p < d {
            p *= base;
            k += 1;
        }
        -k
    }
}

#[test]
fn floor_logarithms_agree_with_integer_search() {
    bounded::within(60, || {
        for n in 1..=250u128 {
            for d in 1..=250u128 {
                let q = rat(i128::try_from(n).unwrap(), i128::try_from(d).unwrap());
                assert_eq!(floor_log2(&q), Some(floor_log(2, n, d)), "log2 {n}/{d}");
                assert_eq!(floor_log10(&q), Some(floor_log(10, n, d)), "log10 {n}/{d}");
            }
        }
        for k in -1_100..=1_100i64 {
            let p = Rat::pow2(k);
            assert_eq!(floor_log2(&p), Some(k), "log2 2^{k}");
            assert_eq!(
                floor_log2(&p.mul(&rat(3, 4))),
                Some(k - 1),
                "log2 3/4 2^{k}"
            );
            assert_eq!(floor_log2(&p.mul(&rat(7, 4))), Some(k), "log2 7/4 2^{k}");
        }
        for k in -340..=340i64 {
            let p = Rat::pow10(k);
            assert_eq!(floor_log10(&p), Some(k), "log10 10^{k}");
            assert_eq!(
                floor_log10(&p.mul(&rat(99, 100))),
                Some(k - 1),
                "log10 0.99 10^{k}"
            );
            assert_eq!(
                floor_log10(&p.mul(&rat(11, 10))),
                Some(k),
                "log10 1.1 10^{k}"
            );
        }
        for q in [Rat::zero(), rat(-1, 1), rat(-1, 3), rat(-1_000, 1)] {
            assert_eq!(floor_log2(&q), None, "log2 {q:?}");
            assert_eq!(floor_log10(&q), None, "log10 {q:?}");
        }
    });
}
