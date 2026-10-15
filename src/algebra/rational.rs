//! Exact fractions for the algebra trainer.

use std::fmt;
use std::ops::{Add, Div, Mul, Neg, Sub};

/// Largest numerator or denominator kept; anything beyond becomes the overflow marker.
/// Products of two in-range values fit easily into `i128`, so no operation can wrap.
const LIMIT: i128 = 1_000_000_000_000_000;

/// An exact fraction in lowest terms with a positive denominator.
///
/// Arithmetic never panics: a result whose numerator or denominator would exceed
/// `LIMIT` becomes an overflow marker that propagates through later operations (like NaN).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rational {
    num: i128,
    /// Positive, or 0 for the overflow marker.
    den: i128,
}

pub fn gcd(a: i128, b: i128) -> i128 {
    let (mut a, mut b) = (a.abs(), b.abs());
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

/// Least common multiple of two positive numbers.
pub fn lcm(a: i128, b: i128) -> i128 {
    a / gcd(a, b) * b
}

impl Rational {
    pub const ZERO: Self = Self { num: 0, den: 1 };
    pub const ONE: Self = Self { num: 1, den: 1 };
    const OVERFLOW: Self = Self { num: 0, den: 0 };

    pub fn new(num: i128, den: i128) -> Self {
        if den == 0 {
            return Self::OVERFLOW;
        }
        let g = gcd(num, den);
        let (num, den) = if den < 0 { (-num / g, -den / g) } else { (num / g, den / g) };
        if num.abs() > LIMIT || den > LIMIT {
            Self::OVERFLOW
        } else {
            Self { num, den }
        }
    }

    pub fn int(n: i128) -> Self {
        Self::new(n, 1)
    }

    pub fn numer(self) -> i128 {
        self.num
    }

    pub fn denom(self) -> i128 {
        self.den
    }

    pub fn is_overflow(self) -> bool {
        self.den == 0
    }

    pub fn is_zero(self) -> bool {
        self == Self::ZERO
    }

    pub fn is_one(self) -> bool {
        self == Self::ONE
    }

    pub fn is_integer(self) -> bool {
        self.den == 1
    }

    pub fn is_negative(self) -> bool {
        self.num < 0
    }

    pub fn abs(self) -> Self {
        Self { num: self.num.abs(), ..self }
    }

    /// `1 / self`; the reciprocal of zero is the overflow marker.
    pub fn recip(self) -> Self {
        Self::new(self.den, self.num)
    }

    pub fn pow(self, exponent: u32) -> Self {
        (0..exponent).fold(Self::ONE, |acc, _| acc * self)
    }

    pub fn to_f64(self) -> f64 {
        self.num as f64 / self.den as f64
    }

    /// The exact square root, if numerator and denominator are perfect squares.
    pub fn sqrt(self) -> Option<Self> {
        if self.is_negative() || self.is_overflow() {
            return None;
        }
        Some(Self::new(isqrt(self.num)?, isqrt(self.den)?))
    }
}

fn isqrt(n: i128) -> Option<i128> {
    let root = (n as f64).sqrt().round() as i128;
    (root - 1..=root + 1).find(|&r| r >= 0 && r * r == n)
}

impl Add for Rational {
    type Output = Self;
    fn add(self, other: Self) -> Self {
        if self.is_overflow() || other.is_overflow() {
            return Self::OVERFLOW;
        }
        Self::new(self.num * other.den + other.num * self.den, self.den * other.den)
    }
}

impl Sub for Rational {
    type Output = Self;
    fn sub(self, other: Self) -> Self {
        self + -other
    }
}

impl Neg for Rational {
    type Output = Self;
    fn neg(self) -> Self {
        Self { num: -self.num, ..self }
    }
}

impl Mul for Rational {
    type Output = Self;
    fn mul(self, other: Self) -> Self {
        if self.is_overflow() || other.is_overflow() {
            return Self::OVERFLOW;
        }
        Self::new(self.num * other.num, self.den * other.den)
    }
}

impl Div for Rational {
    type Output = Self;
    fn div(self, other: Self) -> Self {
        if self.is_overflow() || other.is_overflow() {
            return Self::OVERFLOW;
        }
        Self::new(self.num * other.den, self.den * other.num)
    }
}

/// Uses the typographic minus sign, as everywhere else in the UI.
impl fmt::Display for Rational {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_overflow() {
            return f.write_str("?");
        }
        if self.is_negative() {
            f.write_str("−")?;
        }
        match self.den {
            1 => write!(f, "{}", self.num.abs()),
            den => write!(f, "{}/{den}", self.num.abs()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arithmetic_stays_reduced() {
        let half = Rational::new(1, 2);
        let third = Rational::new(-2, -6);
        assert_eq!(half + third, Rational::new(5, 6));
        assert_eq!(half - Rational::ONE, Rational::new(-1, 2));
        assert_eq!(Rational::new(4, -6), Rational::new(-2, 3));
        assert_eq!((half / third).to_string(), "3/2");
        assert_eq!(Rational::int(-7).to_string(), "−7");
        assert_eq!(Rational::new(9, 4).sqrt(), Some(Rational::new(3, 2)));
        assert_eq!(Rational::int(2).sqrt(), None);
    }

    #[test]
    fn overflow_propagates_instead_of_panicking() {
        let big = Rational::int(LIMIT);
        let product = big * big;
        assert!(product.is_overflow());
        assert!((product + Rational::ONE).is_overflow());
        assert!((product * Rational::ZERO).is_overflow());
        assert!(Rational::ZERO.recip().is_overflow());
        assert_eq!(product.to_string(), "?");
    }
}
