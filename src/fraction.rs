use std::fmt;

/// Largest denominator we visualize; beyond this the pieces become too small to read.
pub const MAX_DENOMINATOR: u32 = 100;
/// Largest number of whole units drawn next to the fractional part.
pub const MAX_WHOLES: u32 = 4;

/// A non-negative mixed fraction in normalized form (`numerator < denominator`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MixedFraction {
    pub wholes: u32,
    pub numerator: u32,
    pub denominator: u32,
}

/// One drawn shape (pie, board, triangle or cube) split into `denominator` groups,
/// of which the first `filled` are highlighted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Unit {
    pub filled: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FractionError {
    ZeroDenominator,
    DenominatorTooLarge,
    TooManyWholes,
}

impl fmt::Display for FractionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroDenominator => write!(f, "Der Nenner darf nicht 0 sein."),
            Self::DenominatorTooLarge => write!(f, "Der Nenner darf höchstens {MAX_DENOMINATOR} sein."),
            Self::TooManyWholes => write!(f, "Es werden höchstens {MAX_WHOLES} Ganze angezeigt."),
        }
    }
}

impl std::error::Error for FractionError {}

impl MixedFraction {
    /// Builds a normalized fraction from raw user input.
    ///
    /// Returns `Ok(None)` when nothing was entered (all zero). Signs are ignored and
    /// improper fractions are converted into wholes plus a proper remainder.
    pub fn from_input(integer: i32, numerator: i32, denominator: i32) -> Result<Option<Self>, FractionError> {
        let (integer, numerator, denominator) = (integer.unsigned_abs(), numerator.unsigned_abs(), denominator.unsigned_abs());

        let denominator = match (numerator, denominator) {
            (0, 0) if integer == 0 => return Ok(None),
            (0, 0) => 1,
            (_, 0) => return Err(FractionError::ZeroDenominator),
            (_, d) if d > MAX_DENOMINATOR => return Err(FractionError::DenominatorTooLarge),
            (_, d) => d,
        };

        let wholes = integer + numerator / denominator;
        if wholes > MAX_WHOLES {
            return Err(FractionError::TooManyWholes);
        }

        Ok(Some(Self { wholes, numerator: numerator % denominator, denominator }))
    }

    /// The units to draw: one completely filled unit per whole, followed by the
    /// fractional remainder (also shown when it is `0/d` without any wholes).
    pub fn units(self) -> Vec<Unit> {
        let whole = Unit { filled: self.denominator };
        let mut units = vec![whole; self.wholes as usize];
        if self.numerator > 0 || self.wholes == 0 {
            units.push(Unit { filled: self.numerator });
        }
        units
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_input_shows_nothing() {
        assert_eq!(MixedFraction::from_input(0, 0, 0), Ok(None));
    }

    #[test]
    fn improper_fraction_is_normalized() {
        let f = MixedFraction::from_input(1, 7, 3).unwrap().unwrap();
        assert_eq!(f, MixedFraction { wholes: 3, numerator: 1, denominator: 3 });
        assert_eq!(f.units().len(), 4);
        assert_eq!(f.units()[3], Unit { filled: 1 });
    }

    #[test]
    fn integer_only_uses_single_part_units() {
        let f = MixedFraction::from_input(2, 0, 0).unwrap().unwrap();
        assert_eq!(f.units(), vec![Unit { filled: 1 }; 2]);
    }

    #[test]
    fn exact_wholes_have_no_empty_remainder() {
        let f = MixedFraction::from_input(0, 8, 4).unwrap().unwrap();
        assert_eq!(f.units(), vec![Unit { filled: 4 }; 2]);
    }

    #[test]
    fn zero_numerator_shows_one_empty_unit() {
        let f = MixedFraction::from_input(0, 0, 5).unwrap().unwrap();
        assert_eq!(f.units(), vec![Unit { filled: 0 }]);
    }

    #[test]
    fn invalid_input_is_rejected() {
        assert_eq!(MixedFraction::from_input(0, 3, 0), Err(FractionError::ZeroDenominator));
        assert_eq!(MixedFraction::from_input(0, 1, 101), Err(FractionError::DenominatorTooLarge));
        assert_eq!(MixedFraction::from_input(5, 0, 0), Err(FractionError::TooManyWholes));
    }
}
