//! `Fraction` arithmetic tests.

use super::Fraction;

#[test]
fn checked_ops_are_exact_and_reduce_before_the_range_check() {
    // Intermediate products past u32 are fine as long as the reduced result fits.
    let tiny = Fraction::new(1, 65_536);
    assert_eq!(
        tiny.checked_mul(Fraction::new(65_536, 1)),
        Some(Fraction::one())
    );
    assert_eq!(tiny.checked_add(tiny), Some(Fraction::new(1, 32_768)));
}

#[test]
fn checked_ops_return_none_instead_of_a_wrong_ratio() {
    // The old saturating version clamped numerator and denominator separately,
    // so 1/65536 * 1/65536 came out as 1/u32::MAX: a different value.
    let tiny = Fraction::new(1, 65_536);
    assert_eq!(tiny.checked_mul(tiny), None);
    let a = Fraction::new(1, 4_294_967_291);
    let b = Fraction::new(1, 4_294_967_279);
    assert_eq!(a.checked_add(b), None);
}

#[test]
fn saturating_add_clamps_to_the_largest_duration() {
    let a = Fraction::new(1, 4_294_967_291);
    let b = Fraction::new(1, 4_294_967_279);
    assert_eq!(a.saturating_add(b), Fraction::MAX);
    assert_eq!(
        Fraction::new(1, 4).saturating_add(Fraction::new(1, 4)),
        Fraction::new(1, 2)
    );
}
