//! Step-by-step algebra: simplifying, expanding and factoring terms, and solving equations.
//!
//! Every mode turns the user's input into a list of [`Step`]s, each with a German
//! explanation and the current expression as tokens with marker colours.

mod equation;
mod factor;
mod parse;
mod rational;
mod simplify;
mod tree;

use std::fmt;

use tree::Sum;

/// Number of marker colours the UI provides; colour indices wrap around.
pub const COLOR_COUNT: usize = 6;
/// German names of the marker colours, in palette order.
const COLOR_NAMES: [&str; COLOR_COUNT] = ["blau", "grün", "orange", "violett", "rot", "türkis"];

fn color_name(index: usize) -> &'static str {
    COLOR_NAMES[index % COLOR_COUNT]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Simplify,
    Expand,
    Factor,
    Solve,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Glyph {
    Text(String),
    /// Numerator over denominator.
    Fraction(String, String),
}

/// One piece of a rendered expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub glyph: Glyph,
    /// Marker colour index for the underline.
    pub color: Option<usize>,
    /// Highlight, because this sign was changed or decided in this step.
    pub sign: bool,
}

pub type Line = Vec<Token>;

#[derive(Debug, Clone, Default)]
pub struct Step {
    pub title: String,
    pub explanation: String,
    /// A warning about signs or a tip; empty if there is none.
    pub hint: String,
    pub line: Line,
    /// Equivalence transformation applied to both sides, e.g. `| −2x`.
    pub operation: String,
}

impl Step {
    fn new(title: impl Into<String>, explanation: impl Into<String>, line: Line) -> Self {
        Self { title: title.into(), explanation: explanation.into(), line, ..Self::default() }
    }

    fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = hint.into();
        self
    }

    fn with_operation(mut self, operation: impl Into<String>) -> Self {
        self.operation = operation.into();
        self
    }
}

impl From<simplify::Stage> for Step {
    fn from(stage: simplify::Stage) -> Self {
        Step::new(stage.title, stage.explanation, stage.sum.line()).with_hint(stage.hint)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AlgebraError {
    Empty,
    TooLong,
    UnexpectedChar(char),
    NumberTooLarge,
    InvalidExponent,
    ExponentTooLarge,
    TooDeep,
    MissingClose,
    UnexpectedClose,
    MissingOperand,
    UnexpectedEnd,
    MisplacedNumber,
    Malformed,
    UnexpectedEquals,
    MissingEquals,
    TooManyEquals,
    EmptySide,
    DivisionByZero,
    DivisionByVariable,
    TooLarge,
    NumbersTooLarge,
    MultipleVariables(String),
    DegreeTooHigh,
}

impl fmt::Display for AlgebraError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("Bitte etwas eingeben."),
            Self::TooLong => f.write_str("Die Eingabe ist zu lang (höchstens 200 Zeichen)."),
            Self::UnexpectedChar(c) => write!(f, "Das Zeichen «{c}» kenne ich nicht."),
            Self::NumberTooLarge => f.write_str("Zahlen dürfen höchstens 7 Ziffern haben."),
            Self::InvalidExponent => f.write_str("Nach ^ muss eine ganze Zahl als Hochzahl stehen, z. B. x^2."),
            Self::ExponentTooLarge => f.write_str("Hochzahlen grösser als 9 werden nicht unterstützt."),
            Self::TooDeep => f.write_str("Zu viele verschachtelte Klammern."),
            Self::MissingClose => f.write_str("Eine Klammer wurde geöffnet, aber nicht geschlossen."),
            Self::UnexpectedClose => f.write_str("Eine Klammer wird geschlossen, die nie geöffnet wurde."),
            Self::MissingOperand => f.write_str("Zwischen zwei Rechenzeichen fehlt eine Zahl oder Variable."),
            Self::UnexpectedEnd => f.write_str("Die Eingabe hört mitten drin auf – es fehlt noch etwas."),
            Self::MisplacedNumber => f.write_str("Eine Zahl steht an einer seltsamen Stelle. Schreibe Zahlen vor die Variable (2x statt x2)."),
            Self::Malformed => f.write_str("Die Eingabe enthält einen Fehler."),
            Self::UnexpectedEquals => f.write_str("Hier wird ein Term erwartet, keine Gleichung (ohne =)."),
            Self::MissingEquals => f.write_str("Eine Gleichung braucht ein Gleichheitszeichen (=)."),
            Self::TooManyEquals => f.write_str("Bitte nur ein Gleichheitszeichen verwenden."),
            Self::EmptySide => f.write_str("Links und rechts vom = muss etwas stehen."),
            Self::DivisionByZero => f.write_str("Durch 0 kann man nicht teilen."),
            Self::DivisionByVariable => f.write_str("Teilen durch Variablen wird nicht unterstützt – nur durch Zahlen."),
            Self::TooLarge => f.write_str("Der Term ist zu gross, um ihn Schritt für Schritt aufzuschreiben."),
            Self::NumbersTooLarge => f.write_str("Die Zahlen werden zu gross."),
            Self::MultipleVariables(names) => write!(f, "Die Gleichung darf nur eine Variable enthalten (gefunden: {names})."),
            Self::DegreeTooHigh => f.write_str("Es werden nur lineare und quadratische Gleichungen gelöst (höchstens x²)."),
        }
    }
}

impl std::error::Error for AlgebraError {}

/// All steps for `input` in the given mode; the last step shows the result.
pub fn steps(mode: Mode, input: &str) -> Result<Vec<Step>, AlgebraError> {
    match mode {
        Mode::Simplify | Mode::Expand => simplify_steps(input),
        Mode::Factor => factor::steps(input),
        Mode::Solve => equation::steps(input),
    }
}

fn simplify_steps(input: &str) -> Result<Vec<Step>, AlgebraError> {
    let (stages, terms) = simplify::normalize(Sum::from_expr(&parse::expression(input)?)?)?;
    let mut steps: Vec<Step> = stages.into_iter().map(Step::from).collect();
    let explanation = if steps.is_empty() {
        "Hier gibt es nichts zu vereinfachen – der Term ist schon so einfach wie möglich."
    } else {
        "Der Term ist vollständig vereinfacht."
    };
    steps.push(Step::new("Ergebnis", explanation, Sum::from_terms(terms).line()));
    Ok(steps)
}

const SIMPLIFY_EXAMPLES: &[&str] = &[
    "3x + 2y - 5x + 4 - y + 7",
    "2x² + 3x - x² + 5 - 7x",
    "4a - 2b + 3a·2b - a + 5b",
    "-3m + 4n - (2m - 5n)",
    "1/2x + 3/4x - 2",
];
const EXPAND_EXAMPLES: &[&str] = &[
    "3(x + 4)",
    "-2(a - 4b)",
    "(x + 3)(x - 2)",
    "(a + b)^2",
    "x(2x - 1) - 3(x + 4)",
    "(2x - 1)(x + 5) - (x - 3)^2",
];
const FACTOR_EXAMPLES: &[&str] = &["6x + 9", "4x² - 8x", "-3a - 6b", "12a²b + 18ab² - 6ab", "x³ - 2x² + x"];
const SOLVE_EXAMPLES: &[&str] = &[
    "3x + 4 = 19",
    "5x - 7 = 2x + 8",
    "3(x - 2) + 4 = 2x - 5",
    "-4x + 3 = 2(x - 6)",
    "x/2 + 1 = x/3 + 2",
    "x² - 5x + 6 = 0",
    "2x² = 18",
    "x² + 3x = 0",
];

/// The `index`-th example for a mode (wrapping around).
pub fn example(mode: Mode, index: usize) -> &'static str {
    let list = match mode {
        Mode::Simplify => SIMPLIFY_EXAMPLES,
        Mode::Expand => EXPAND_EXAMPLES,
        Mode::Factor => FACTOR_EXAMPLES,
        Mode::Solve => SOLVE_EXAMPLES,
    };
    list[index % list.len()]
}

#[cfg(test)]
mod tests {
    use super::tree::line_text;
    use super::*;

    const MODES: [Mode; 4] = [Mode::Simplify, Mode::Expand, Mode::Factor, Mode::Solve];

    #[test]
    fn every_example_works() {
        for mode in MODES {
            for index in 0..10 {
                let input = example(mode, index);
                let steps = steps(mode, input).unwrap_or_else(|e| panic!("{input}: {e}"));
                assert!(!steps.is_empty(), "{input}");
                assert!(steps.iter().all(|s| !line_text(&s.line).contains('?')), "{input}");
            }
        }
    }

    #[test]
    fn simplify_ends_with_the_result() {
        let steps = steps(Mode::Simplify, "3x + 2y - 5x + 4 - y + 7").unwrap();
        let last = steps.last().unwrap();
        assert_eq!(last.title, "Ergebnis");
        assert_eq!(line_text(&last.line), "−2x + y + 11");

        let untouched = super::steps(Mode::Simplify, "x + 1").unwrap();
        assert_eq!(untouched.len(), 1);
    }

    #[test]
    fn errors_are_reported_in_every_mode() {
        for mode in MODES {
            assert!(steps(mode, "3x + ").is_err());
            assert!(steps(mode, "(x + 1").is_err());
        }
        assert_eq!(steps(Mode::Simplify, "x = 1").unwrap_err(), AlgebraError::UnexpectedEquals);
        assert_eq!(steps(Mode::Expand, "x/(x + 1)").unwrap_err(), AlgebraError::DivisionByVariable);
    }
}
