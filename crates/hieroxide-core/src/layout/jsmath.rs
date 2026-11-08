//! `Math.min`, `Math.max` and the array helpers of `src/util.js`, with JavaScript's
//! semantics: NaN wins, and -0 is smaller than +0. Rust's `f64::min` and `f64::max`
//! return the other operand when one is NaN, which JS does not.

/// `Math.max(a, b)`.
pub fn js_max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a == 0.0 && b == 0.0 {
        if a.is_sign_positive() { a } else { b }
    } else if a > b {
        a
    } else {
        b
    }
}

/// `Math.min(a, b)`.
pub fn js_min(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a == 0.0 && b == 0.0 {
        if a.is_sign_negative() { a } else { b }
    } else if a < b {
        a
    } else {
        b
    }
}

/// `getSum` of `util.js`: `arr.reduce((acc, x) => acc + x, 0)`.
pub fn get_sum(it: impl IntoIterator<Item = f64>) -> f64 {
    it.into_iter().fold(0.0, |acc, x| acc + x)
}

/// `getMax` of `util.js`: `arr.reduce((acc, x) => Math.max(acc, x), -Infinity)`.
pub fn get_max(it: impl IntoIterator<Item = f64>) -> f64 {
    it.into_iter().fold(f64::NEG_INFINITY, js_max)
}

/// `getMin` of `util.js`: `arr.reduce((acc, x) => Math.min(acc, x), Infinity)`.
pub fn get_min(it: impl IntoIterator<Item = f64>) -> f64 {
    it.into_iter().fold(f64::INFINITY, js_min)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nan_and_signed_zero() {
        assert!(js_max(f64::NAN, 1.0).is_nan());
        assert!(js_max(1.0, f64::NAN).is_nan());
        assert!(js_min(f64::NAN, 1.0).is_nan());
        assert!(js_max(-0.0, 0.0).is_sign_positive());
        assert!(js_max(0.0, -0.0).is_sign_positive());
        assert!(js_min(0.0, -0.0).is_sign_negative());
        assert!(js_min(-0.0, 0.0).is_sign_negative());
        assert_eq!(js_max(1.0, 2.0), 2.0);
        assert_eq!(js_min(1.0, 2.0), 1.0);
    }

    #[test]
    fn reductions() {
        assert_eq!(get_max(std::iter::empty()), f64::NEG_INFINITY);
        assert_eq!(get_min(std::iter::empty()), f64::INFINITY);
        assert_eq!(get_sum(std::iter::empty()), 0.0);
        assert!(get_max([1.0, f64::NAN, 3.0]).is_nan());
        // 0 + (-0) is +0
        assert!(get_sum([-0.0]).is_sign_positive());
    }
}
