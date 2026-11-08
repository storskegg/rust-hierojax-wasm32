//! JavaScript number semantics, on `f64` only, with no dependencies.
//!
//! Upstream hierojax builds its output from `Number.prototype.toFixed`,
//! `String(number)`, `Math.round` and `Number(string)`. Layout is plain `f64`
//! arithmetic, so the only places where a port can quietly drift are these four
//! conversions. Each is reproduced here exactly:
//!
//! * [`to_fixed`]: `x.toFixed(digits)` for `digits` in `0..=100`. Ties go to the
//!   larger `n` (so `(2.5).toFixed(0)` is `"3"` and `(-2.5).toFixed(0)` is
//!   `"-3"`), decided on the exact binary value (so `(1.005).toFixed(2)` is
//!   `"1.00"`). `-0` gives `"0"`, tiny negatives give `"-0.00"`, and
//!   `abs >= 1e21` falls back to [`number_to_string`].
//! * [`number_to_string`]: `String(x)`, i.e. the shortest round-trip digits laid
//!   out by the ECMAScript exponent rules.
//! * [`js_round`]: `Math.round`, i.e. ties toward +infinity, `-0` for
//!   `[-0.5, -0]`, and no `x + 0.5` rounding trap.
//! * [`number_from_str`]: `Number(string)` for strings: whitespace trimming
//!   (JS's whitespace set, not Rust's), the empty string as `0`, decimal
//!   literals, `Infinity`, signed forms, and the unsigned `0x`/`0o`/`0b` forms.
//!
//! The writers append to a caller's `&mut String`, so a caller that keeps one
//! buffer allocates nothing. The one exception is `to_fixed` with more than 17
//! digits, which needs a small big-integer and allocates; nothing in hierojax
//! asks for more than 3.
//!
//! Not covered: `Number(x)` for non-string `x` (`null`, `undefined`, booleans,
//! arrays), numeric separators (JS rejects them in `Number()`, and so do we, as
//! `NaN`), and `toFixed`'s argument coercion (`digits` is already a `u32`;
//! values above 100 panic where JS throws a `RangeError`).
//!
//! `number_to_string` uses the standard library's `{:e}` formatter for the
//! shortest digits. It does not allocate, but it pulls the float formatting
//! code into a wasm build; replacing it with a hand-written shortest-digits
//! routine later would not change behaviour, and the test against real JS
//! would catch any slip.

use core::fmt::Write as _;

/// `Number.prototype.toFixed`, appended to `out`.
///
/// # Panics
///
/// If `digits > 100` (JS throws a `RangeError` there).
pub fn to_fixed(x: f64, digits: u32, out: &mut String) {
    assert!(digits <= 100, "toFixed digits must be in 0..=100");
    if x.is_nan() {
        out.push_str("NaN");
        return;
    }
    // `-0 < 0` is false, so -0 takes the non-negative path and prints "0".
    let neg = x < 0.0;
    let ax = x.abs();
    if neg {
        out.push('-');
    }
    if ax >= 1e21 {
        // Includes infinity.
        number_to_string(ax, out);
        return;
    }

    // ax is finite, below 1e21 (< 2^70), and ax = m * 2^e exactly.
    let (m, e) = decompose(ax);

    if digits <= 17 {
        // m < 2^53 and 10^17 < 2^57, so every product below fits in a u128.
        let p10 = 10u128.pow(digits);
        let n: u128 = if e >= 0 {
            // ax is an integer below 2^70, so e <= 17 here.
            ((m as u128) << e) * p10
        } else {
            let k = (-e) as u32;
            let prod = (m as u128) * p10; // < 2^110
            if k >= 128 {
                0
            } else {
                // floor(prod / 2^k + 1/2): exact, ties go up.
                (prod + (1u128 << (k - 1))) >> k
            }
        };
        let mut buf = [0u8; 40];
        let mut i = buf.len();
        let mut v = n;
        loop {
            i -= 1;
            buf[i] = b'0' + (v % 10) as u8;
            v /= 10;
            if v == 0 {
                break;
            }
        }
        push_fixed(&buf[i..], digits as usize, out);
    } else {
        let dec = big_fixed_digits(m, e, digits);
        push_fixed(&dec, digits as usize, out);
    }
}

/// Lay out the decimal digits of `n = x * 10^f` with the point `f` places from
/// the right, padding with zeros as the spec does.
fn push_fixed(dec: &[u8], f: usize, out: &mut String) {
    let l = dec.len();
    let s = ascii;
    if f == 0 {
        out.push_str(s(dec));
    } else if l <= f {
        out.push_str("0.");
        for _ in 0..(f - l) {
            out.push('0');
        }
        out.push_str(s(dec));
    } else {
        out.push_str(s(&dec[..l - f]));
        out.push('.');
        out.push_str(s(&dec[l - f..]));
    }
}

/// The input is always ASCII digits and punctuation written by this module.
fn ascii(b: &[u8]) -> &str {
    core::str::from_utf8(b).expect("ascii")
}

/// Is `x` exactly halfway between two `k`-digit decimals, i.e. does its exact
/// decimal expansion have exactly `k + 1` significant digits? (`n` is the
/// decimal exponent with `x = 0.DIGITS * 10^n`.) Only fractional doubles can
/// be: an integer tie would need an odd part above 2^53.
fn is_exact_tie(x: f64, n: i32, k: i32) -> bool {
    let (m, e) = decompose(x);
    let e = e + m.trailing_zeros() as i32;
    e < 0 && -e == k + 1 - n
}

/// `x = m * 2^e` exactly, for finite non-negative `x`.
fn decompose(x: f64) -> (u64, i32) {
    let bits = x.to_bits();
    let exp = ((bits >> 52) & 0x7ff) as i32;
    let frac = bits & ((1u64 << 52) - 1);
    if exp == 0 {
        (frac, -1074)
    } else {
        (frac | (1u64 << 52), exp - 1075)
    }
}

// A small big-integer (little-endian u32 limbs), only for `digits > 17`.

fn big_fixed_digits(m: u64, e: i32, digits: u32) -> Vec<u8> {
    let mut v: Vec<u32> = vec![m as u32, (m >> 32) as u32];
    let mut d = digits;
    while d > 0 {
        let s = d.min(9);
        big_mul_small(&mut v, 10u32.pow(s));
        d -= s;
    }
    if e >= 0 {
        big_shl(&mut v, e as usize);
    } else {
        let k = (-e) as usize;
        big_add_bit(&mut v, k - 1);
        big_shr(&mut v, k);
    }
    big_to_decimal(v)
}

fn big_mul_small(v: &mut Vec<u32>, k: u32) {
    let mut carry = 0u64;
    for limb in v.iter_mut() {
        let t = (*limb as u64) * (k as u64) + carry;
        *limb = t as u32;
        carry = t >> 32;
    }
    if carry != 0 {
        v.push(carry as u32);
    }
}

fn big_shl(v: &mut Vec<u32>, n: usize) {
    let (limbs, bits) = (n / 32, (n % 32) as u32);
    if bits != 0 {
        let mut carry = 0u32;
        for limb in v.iter_mut() {
            let t = (*limb << bits) | carry;
            carry = *limb >> (32 - bits);
            *limb = t;
        }
        if carry != 0 {
            v.push(carry);
        }
    }
    if limbs != 0 {
        v.splice(0..0, core::iter::repeat_n(0u32, limbs));
    }
}

fn big_shr(v: &mut Vec<u32>, n: usize) {
    let (limbs, bits) = (n / 32, (n % 32) as u32);
    if limbs >= v.len() {
        v.clear();
        return;
    }
    v.drain(0..limbs);
    if bits != 0 {
        let len = v.len();
        for i in 0..len {
            let hi = if i + 1 < len {
                v[i + 1] << (32 - bits)
            } else {
                0
            };
            v[i] = (v[i] >> bits) | hi;
        }
    }
}

fn big_add_bit(v: &mut Vec<u32>, bit: usize) {
    let (mut i, b) = (bit / 32, bit % 32);
    if v.len() <= i {
        v.resize(i + 1, 0);
    }
    let (mut t, mut over) = v[i].overflowing_add(1u32 << b);
    v[i] = t;
    while over {
        i += 1;
        if i == v.len() {
            v.push(1);
            return;
        }
        (t, over) = v[i].overflowing_add(1);
        v[i] = t;
    }
}

fn big_to_decimal(mut v: Vec<u32>) -> Vec<u8> {
    let mut chunks: Vec<u32> = Vec::new(); // base 1e9, least significant first
    while v.iter().any(|&l| l != 0) {
        let mut rem = 0u64;
        for limb in v.iter_mut().rev() {
            let cur = (rem << 32) | (*limb as u64);
            *limb = (cur / 1_000_000_000) as u32;
            rem = cur % 1_000_000_000;
        }
        chunks.push(rem as u32);
    }
    let mut out: Vec<u8> = Vec::new();
    match chunks.split_last() {
        None => out.push(b'0'),
        Some((top, rest)) => {
            out.extend_from_slice(top.to_string().as_bytes());
            for c in rest.iter().rev() {
                out.extend_from_slice(format!("{c:09}").as_bytes());
            }
        }
    }
    out
}

/// A fixed stack buffer for `fmt::Write`, so shortest-digit formatting does not
/// allocate.
struct StackBuf {
    buf: [u8; 40],
    len: usize,
}

impl core::fmt::Write for StackBuf {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let b = s.as_bytes();
        let end = self.len + b.len();
        if end > self.buf.len() {
            return Err(core::fmt::Error);
        }
        self.buf[self.len..end].copy_from_slice(b);
        self.len = end;
        Ok(())
    }
}

/// `String(x)` for a number, appended to `out`.
pub fn number_to_string(x: f64, out: &mut String) {
    if x.is_nan() {
        out.push_str("NaN");
        return;
    }
    if x == 0.0 {
        out.push('0'); // also -0
        return;
    }
    let mut x = x;
    if x < 0.0 {
        out.push('-');
        x = -x;
    }
    if x.is_infinite() {
        out.push_str("Infinity");
        return;
    }
    // Integers below 2^53 print as themselves; skip the float formatter.
    if x < 9007199254740992.0 && x == x.trunc() {
        push_u64(x as u64, out);
        return;
    }

    // Shortest round-trip digits: "d.ddde<exp>" or "de<exp>".
    let mut sb = StackBuf {
        buf: [0; 40],
        len: 0,
    };
    write!(sb, "{x:e}").expect("shortest float fits in 40 bytes");
    let s = core::str::from_utf8(&sb.buf[..sb.len]).expect("ascii");
    let (mant, exp) = s.split_once('e').expect("exponent form");
    let exp: i32 = exp.parse().expect("exponent");
    let mut digits = [0u8; 20];
    let mut k = 0usize;
    for &c in mant.as_bytes() {
        if c != b'.' {
            digits[k] = c;
            k += 1;
        }
    }
    let n = exp + 1; // value = 0.DIGITS * 10^n
    // Rust's shortest-digits routine breaks an exact tie between two equally
    // short, equally close candidates upward; ECMAScript (and V8) pick the even
    // one. A tie needs x to have exactly k+1 significant decimal digits, and
    // the even neighbour must itself round-trip (it may not, next to a power of
    // two, where the interval below x is half as wide).
    if k > 1 && digits[k - 1] & 1 == 1 && is_exact_tie(x, n, k as i32) {
        let mut cand = StackBuf {
            buf: [0; 40],
            len: 0,
        };
        let _ = cand.write_str(ascii(&digits[..1]));
        let _ = cand.write_char('.');
        let _ = cand.write_str(ascii(&digits[1..k - 1]));
        let _ = cand.write_char((digits[k - 1] - 1) as char);
        let _ = write!(cand, "e{exp}");
        if ascii(&cand.buf[..cand.len]).parse::<f64>() == Ok(x) {
            digits[k - 1] -= 1;
        }
    }
    let digits = &digits[..k];
    let k_i = k as i32;
    let ds = ascii;

    if k_i <= n && n <= 21 {
        out.push_str(ds(digits));
        for _ in 0..(n - k_i) {
            out.push('0');
        }
    } else if 0 < n && n <= 21 {
        out.push_str(ds(&digits[..n as usize]));
        out.push('.');
        out.push_str(ds(&digits[n as usize..]));
    } else if -6 < n && n <= 0 {
        out.push_str("0.");
        for _ in 0..(-n) {
            out.push('0');
        }
        out.push_str(ds(digits));
    } else {
        out.push_str(ds(&digits[..1]));
        if k > 1 {
            out.push('.');
            out.push_str(ds(&digits[1..]));
        }
        out.push('e');
        let e = n - 1;
        out.push(if e < 0 { '-' } else { '+' });
        push_u64(e.unsigned_abs() as u64, out);
    }
}

fn push_u64(mut v: u64, out: &mut String) {
    let mut buf = [0u8; 20];
    let mut i = buf.len();
    loop {
        i -= 1;
        buf[i] = b'0' + (v % 10) as u8;
        v /= 10;
        if v == 0 {
            break;
        }
    }
    out.push_str(core::str::from_utf8(&buf[i..]).expect("ascii"));
}

/// `Math.round`.
pub fn js_round(x: f64) -> f64 {
    // NaN, infinities and zeros (with their sign) pass through; at 2^52 and
    // above every double is already an integer.
    if !x.is_finite() || x == 0.0 || x.abs() >= 4503599627370496.0 {
        return x;
    }
    let f = x.floor();
    // x - f is exact here.
    let r = if x - f >= 0.5 { f + 1.0 } else { f };
    if r == 0.0 && x < 0.0 { -0.0 } else { r }
}

/// JS `StrWhiteSpaceChar`: WhiteSpace plus LineTerminator.
fn is_js_space(c: char) -> bool {
    matches!(
        c,
        '\u{9}'
            | '\u{A}'
            | '\u{B}'
            | '\u{C}'
            | '\u{D}'
            | '\u{20}'
            | '\u{A0}'
            | '\u{1680}'
            | '\u{2000}'
            ..='\u{200A}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202F}'
                | '\u{205F}'
                | '\u{3000}'
                | '\u{FEFF}'
    )
}

/// `Number(s)` for a string `s`.
pub fn number_from_str(s: &str) -> f64 {
    let t = s.trim_matches(is_js_space);
    if t.is_empty() {
        return 0.0;
    }
    let b = t.as_bytes();
    if b.len() > 2 && b[0] == b'0' {
        let radix = match b[1] {
            b'x' | b'X' => 16,
            b'o' | b'O' => 8,
            b'b' | b'B' => 2,
            _ => 0,
        };
        if radix != 0 {
            return parse_pow2_radix(&b[2..], radix);
        }
    }
    if b.len() == 2 && b[0] == b'0' && matches!(b[1], b'x' | b'X' | b'o' | b'O' | b'b' | b'B') {
        return f64::NAN;
    }

    // StrDecimalLiteral.
    let mut i = 0;
    let neg = match b[0] {
        b'+' => {
            i = 1;
            false
        }
        b'-' => {
            i = 1;
            true
        }
        _ => false,
    };
    if &b[i..] == b"Infinity" {
        return if neg {
            f64::NEG_INFINITY
        } else {
            f64::INFINITY
        };
    }
    let int_start = i;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    let mut n_digits = i - int_start;
    if i < b.len() && b[i] == b'.' {
        i += 1;
        let f = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        n_digits += i - f;
    }
    if n_digits == 0 {
        return f64::NAN;
    }
    if i < b.len() && (b[i] == b'e' || b[i] == b'E') {
        i += 1;
        if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
            i += 1;
        }
        let e = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        if i == e {
            return f64::NAN;
        }
    }
    if i != b.len() {
        return f64::NAN;
    }
    // Validated: Rust's parser accepts this shape and rounds correctly.
    t.parse::<f64>().unwrap_or(f64::NAN)
}

/// An unsigned integer literal in radix 2, 8 or 16, rounded to nearest, ties to
/// even, with no length limit. Invalid digits, or no digits, give NaN.
fn parse_pow2_radix(digits: &[u8], radix: u32) -> f64 {
    if digits.is_empty() {
        return f64::NAN;
    }
    let bits_per = match radix {
        2 => 1,
        8 => 3,
        _ => 4,
    };
    let mut mant: u64 = 0;
    let mut mant_bits = 0u32; // significant bits held in `mant`
    let mut extra = 0i64; // bits dropped below `mant`
    let mut sticky = false;
    for &c in digits {
        let d = match (c as char).to_digit(radix) {
            Some(d) if c.is_ascii() => d,
            _ => return f64::NAN,
        };
        for bit_i in (0..bits_per).rev() {
            let bit = (d >> bit_i) & 1;
            if mant_bits == 0 && bit == 0 {
                continue;
            }
            if mant_bits < 64 {
                mant = (mant << 1) | bit as u64;
                mant_bits += 1;
            } else {
                sticky |= bit == 1;
                extra += 1;
            }
        }
    }
    if mant_bits == 0 {
        return 0.0;
    }
    let (m53, shift) = if mant_bits <= 53 {
        (mant, 0u32)
    } else {
        let shift = mant_bits - 53;
        let low = mant & ((1u64 << shift) - 1);
        let half = 1u64 << (shift - 1);
        let mut m = mant >> shift;
        if low > half || (low == half && (sticky || m & 1 == 1)) {
            m += 1;
        }
        (m, shift)
    };
    let e = shift as i64 + extra;
    if e > 1023 {
        return f64::INFINITY;
    }
    // m53 <= 2^53 is exact in f64; 2^e is exact for e <= 1023.
    (m53 as f64) * f64::from_bits(((e + 1023) as u64) << 52)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixed(x: f64, d: u32) -> String {
        let mut s = String::new();
        to_fixed(x, d, &mut s);
        s
    }

    fn string(x: f64) -> String {
        let mut s = String::new();
        number_to_string(x, &mut s);
        s
    }

    #[test]
    fn to_fixed_ties_and_exactness() {
        assert_eq!(fixed(2.5, 0), "3");
        assert_eq!(fixed(-2.5, 0), "-3");
        assert_eq!(fixed(0.5, 0), "1");
        assert_eq!(fixed(1.5, 0), "2");
        assert_eq!(fixed(1.005, 2), "1.00"); // the double is below 1.005
        assert_eq!(fixed(0.125, 2), "0.13"); // exact tie goes up
        assert_eq!(fixed(0.375, 2), "0.38");
        assert_eq!(fixed(0.0625, 3), "0.063");
        assert_eq!(fixed(8.345, 2), "8.35");
        assert_eq!(fixed(123.456, 10), "123.4560000000");
    }

    #[test]
    fn to_fixed_signs_and_edges() {
        assert_eq!(fixed(-0.0, 0), "0");
        assert_eq!(fixed(-0.0, 2), "0.00");
        assert_eq!(fixed(0.0, 2), "0.00");
        assert_eq!(fixed(-1e-10, 2), "-0.00");
        assert_eq!(fixed(-0.004, 2), "-0.00");
        assert_eq!(fixed(1e21, 2), "1e+21");
        assert_eq!(fixed(-1e21, 2), "-1e+21");
        assert_eq!(
            fixed(999999999999999900000.0, 2),
            "999999999999999868928.00"
        );
        assert_eq!(fixed(f64::NAN, 2), "NaN");
        assert_eq!(fixed(f64::INFINITY, 2), "Infinity");
        assert_eq!(fixed(f64::NEG_INFINITY, 0), "-Infinity");
        assert_eq!(fixed(5e-324, 3), "0.000");
        assert_eq!(fixed(0.1, 20), "0.10000000000000000555");
        assert_eq!(fixed(1.0, 100).len(), 102);
    }

    #[test]
    fn number_to_string_layouts() {
        assert_eq!(string(0.0), "0");
        assert_eq!(string(-0.0), "0");
        assert_eq!(string(1.0), "1");
        assert_eq!(string(-1.0), "-1");
        assert_eq!(string(0.1), "0.1");
        assert_eq!(string(0.1 + 0.2), "0.30000000000000004");
        assert_eq!(string(1e21), "1e+21");
        assert_eq!(string(1e20), "100000000000000000000");
        assert_eq!(string(123456789012345680000.0), "123456789012345680000");
        assert_eq!(string(1.5e-7), "1.5e-7");
        assert_eq!(string(1e-7), "1e-7");
        assert_eq!(string(0.000001), "0.000001");
        assert_eq!(string(1.2345e25), "1.2345e+25");
        assert_eq!(string(5e-324), "5e-324");
        assert_eq!(string(f64::MAX), "1.7976931348623157e+308");
        assert_eq!(string(9007199254740993.0), "9007199254740992");
        assert_eq!(string(f64::NAN), "NaN");
        assert_eq!(string(f64::NEG_INFINITY), "-Infinity");
        assert_eq!(string(-12.5), "-12.5");
    }

    #[test]
    fn js_round_cases() {
        assert_eq!(js_round(2.5), 3.0);
        assert_eq!(js_round(-2.5), -2.0);
        assert_eq!(js_round(0.5), 1.0);
        assert_eq!(js_round(1.4999999999999998), 1.0);
        assert_eq!(js_round(0.49999999999999994), 0.0);
        assert!(js_round(0.49999999999999994).is_sign_positive());
        assert!(js_round(-0.5).is_sign_negative() && js_round(-0.5) == 0.0);
        assert!(js_round(-0.2).is_sign_negative() && js_round(-0.2) == 0.0);
        assert!(js_round(-0.0).is_sign_negative());
        assert!(js_round(0.0).is_sign_positive());
        assert_eq!(js_round(-0.51), -1.0);
        assert_eq!(js_round(4503599627370495.5), 4503599627370496.0);
        assert_eq!(js_round(f64::INFINITY), f64::INFINITY);
        assert!(js_round(f64::NAN).is_nan());
    }

    #[test]
    fn number_from_str_cases() {
        assert_eq!(number_from_str("12"), 12.0);
        assert_eq!(number_from_str("  0.25\n"), 0.25);
        assert_eq!(number_from_str("\u{a0}\u{feff}7\u{2028}"), 7.0);
        assert_eq!(number_from_str(""), 0.0);
        assert_eq!(number_from_str(" \t "), 0.0);
        assert_eq!(number_from_str("0x1F"), 31.0);
        assert_eq!(number_from_str("0b101"), 5.0);
        assert_eq!(number_from_str("0o17"), 15.0);
        assert!(number_from_str("-0x1F").is_nan());
        assert!(number_from_str("0x").is_nan());
        assert!(number_from_str("0xG").is_nan());
        assert_eq!(number_from_str("Infinity"), f64::INFINITY);
        assert_eq!(number_from_str("-Infinity"), f64::NEG_INFINITY);
        assert!(number_from_str("infinity").is_nan());
        assert!(number_from_str("inf").is_nan());
        assert!(number_from_str("nan").is_nan());
        assert!(number_from_str("NaN").is_nan());
        assert!(number_from_str("1_000").is_nan());
        assert!(number_from_str("1e").is_nan());
        assert!(number_from_str(".").is_nan());
        assert!(number_from_str("1 2").is_nan());
        assert!(number_from_str("\u{85}1").is_nan());
        assert_eq!(number_from_str(".5"), 0.5);
        assert_eq!(number_from_str("5."), 5.0);
        assert_eq!(number_from_str("+.5e1"), 5.0);
        assert!(number_from_str("-0").is_sign_negative());
        assert_eq!(number_from_str("1e1000"), f64::INFINITY);
        assert_eq!(number_from_str("0.07"), 0.07);
        assert_eq!(number_from_str("0x20000000000001"), 9007199254740992.0);
        assert_eq!(number_from_str("0x20000000000003"), 9007199254740996.0);
    }

    /// Compares against real JS: `cargo test -p hieroxide-core jsnum -- --ignored`.
    /// Needs `node` on PATH (or set `NODE`). `JSNUM_CASES` sets the number of
    /// float cases per shard (default 3,000,000 over 4 shards = 12M);
    /// `JSNUM_SHARDS` sets the shard count.
    #[test]
    #[ignore = "spawns node and compares against real JS; slow"]
    fn matches_real_js() {
        use std::io::{BufRead, BufReader};
        use std::process::{Command, Stdio};

        let node = std::env::var("NODE").unwrap_or_else(|_| "node".into());
        let gen_js = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/jsnum/gen.mjs");
        let cases: u64 = std::env::var("JSNUM_CASES")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(3_000_000);
        let shards: u64 = std::env::var("JSNUM_SHARDS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(4);

        let run = |mode: &'static str, shard: u64, n: u64| {
            let node = node.clone();
            std::thread::spawn(move || {
                let mut child = Command::new(&node)
                    .args([
                        gen_js,
                        mode,
                        "--seed",
                        &format!("{}", 1000 + shard),
                        "--shard",
                        &shard.to_string(),
                        "--count",
                        &n.to_string(),
                    ])
                    .stdout(Stdio::piped())
                    .spawn()
                    .expect("spawn node");
                let mut rd = BufReader::new(child.stdout.take().unwrap());
                let mut line = String::new();
                let (mut checked, mut bad) = (0u64, Vec::<String>::new());
                loop {
                    line.clear();
                    if rd.read_line(&mut line).expect("read") == 0 {
                        break;
                    }
                    let l = line.trim_end_matches('\n');
                    let f: Vec<&str> = l.split('\t').collect();
                    match mode {
                        "floats" => {
                            // bits, toFixed 0..3, toFixed(d) for a random d, d, String(x), Math.round bits
                            let x = f64::from_bits(u64::from_str_radix(f[0], 16).unwrap());
                            for d in 0..4u32 {
                                let got = fixed(x, d);
                                checked += 1;
                                if got != f[1 + d as usize] && bad.len() < 20 {
                                    bad.push(format!(
                                        "toFixed({x:e} bits {}, {d}): got {got} want {}",
                                        f[0],
                                        f[1 + d as usize]
                                    ));
                                }
                            }
                            let d: u32 = f[6].parse().unwrap();
                            let got = fixed(x, d);
                            checked += 1;
                            if got != f[5] && bad.len() < 20 {
                                bad.push(format!(
                                    "toFixed(bits {}, {d}): got {got} want {}",
                                    f[0], f[5]
                                ));
                            }
                            let got = string(x);
                            checked += 1;
                            if got != f[7] && bad.len() < 20 {
                                bad.push(format!("String(bits {}): got {got} want {}", f[0], f[7]));
                            }
                            let got = js_round(x).to_bits();
                            let want = u64::from_str_radix(f[8], 16).unwrap();
                            checked += 1;
                            let both_nan =
                                f64::from_bits(got).is_nan() && f64::from_bits(want).is_nan();
                            if got != want && !both_nan && bad.len() < 20 {
                                bad.push(format!(
                                    "Math.round(bits {}): got {got:016x} want {want:016x}",
                                    f[0]
                                ));
                            }
                        }
                        _ => {
                            // hex(utf8 of string), bits of Number(string)
                            let bytes: Vec<u8> = (0..f[0].len() / 2)
                                .map(|i| u8::from_str_radix(&f[0][2 * i..2 * i + 2], 16).unwrap())
                                .collect();
                            let s = String::from_utf8(bytes).unwrap();
                            let got = number_from_str(&s);
                            let want = f64::from_bits(u64::from_str_radix(f[1], 16).unwrap());
                            checked += 1;
                            let same =
                                got.to_bits() == want.to_bits() || (got.is_nan() && want.is_nan());
                            if !same && bad.len() < 20 {
                                bad.push(format!("Number({s:?}): got {got:?} want {want:?}"));
                            }
                        }
                    }
                }
                assert!(child.wait().unwrap().success(), "gen.mjs failed");
                (checked, bad)
            })
        };

        let mut handles = Vec::new();
        for shard in 0..shards {
            handles.push(("floats", cases, run("floats", shard, cases)));
        }
        handles.push(("strings", 2_000_000, run("strings", 99, 2_000_000)));
        let mut all_bad = Vec::new();
        let mut floats = 0;
        for (mode, n, h) in handles {
            let (checked, bad) = h.join().unwrap();
            eprintln!(
                "{mode}: {n} cases, {checked} comparisons, {} mismatches",
                bad.len()
            );
            if mode == "floats" {
                floats += n;
            }
            all_bad.extend(bad);
        }
        eprintln!(
            "total float cases (each checked by toFixed 0..3 + one random digits, String, Math.round): {floats}"
        );
        assert!(
            all_bad.is_empty(),
            "mismatches against real JS:\n{}",
            all_bad.join("\n")
        );
    }
}
