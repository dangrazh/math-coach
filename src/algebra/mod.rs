//! Step-by-step algebra: simplifying, expanding and factoring terms, solving equations,
//! and cancelling and expanding fractions.
//!
//! Every mode turns the user's input into a list of [`Step`]s, each with a German
//! explanation and the current expression as tokens with marker colours.

mod equation;
mod factor;
mod fractions;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Mode {
    Simplify,
    Expand,
    Factor,
    Solve,
    /// Cancelling a fraction of numbers.
    Reduce,
    /// Expanding a fraction of numbers.
    Extend,
    ReduceVariables,
    ExtendVariables,
    ReducePowers,
    /// Calculating with two fractions; the four modes only differ in their examples.
    Multiply,
    Divide,
    Add,
    Subtract,
    /// Longer calculations with several operators and brackets.
    Combined,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Glyph {
    Text(String),
    /// Numerator over denominator.
    Fraction(String, String),
    /// A fraction whose numerator and denominator are themselves coloured lines.
    Stacked(Line, Line),
}

/// One piece of a rendered expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub glyph: Glyph,
    /// Marker colour index for the underline.
    pub color: Option<usize>,
    /// Highlight, because this sign was changed or decided in this step.
    pub sign: bool,
    /// Crossed out, because this factor was cancelled.
    pub struck: bool,
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
    MissingFractionBar,
    TooManyFractionBars,
    EmptyFractionPart,
    NotInteger,
    VariablesNotAllowed,
    MissingExtension,
    ExtendByZero,
    /// The target denominator (first) is no multiple of the denominator (second).
    NotAMultiple(String, String),
    MissingOperator,
    SlashBetweenFractions,
    TooManyOperands,
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
            Self::MissingFractionBar => f.write_str("Bitte einen Bruch mit Bruchstrich eingeben, z. B. 12/18."),
            Self::TooManyFractionBars => {
                f.write_str("Bitte nur einen Bruchstrich (/) verwenden. Setze Zähler und Nenner bei Bedarf in Klammern.")
            }
            Self::EmptyFractionPart => f.write_str("Über und unter dem Bruchstrich muss etwas stehen."),
            Self::NotInteger => f.write_str("Bitte nur ganze Zahlen verwenden – keine Dezimalzahlen oder Brüche im Zähler und Nenner."),
            Self::VariablesNotAllowed => f.write_str("Hier bitte nur Zahlen eingeben. Für Brüche mit Variablen gibt es eigene Tabs."),
            Self::MissingExtension => {
                f.write_str("Schreibe dazu, womit erweitert wird («3/4 mit 5») oder auf welchen Nenner («3/4 auf 20»).")
            }
            Self::ExtendByZero => f.write_str("Mit 0 darf man nicht erweitern – im Nenner stünde sonst 0."),
            Self::NotAMultiple(target, denominator) => {
                write!(f, "{target} ist kein Vielfaches von {denominator} – auf diesen Nenner kann man nicht erweitern.")
            }
            Self::MissingOperator => f.write_str("Zwischen den beiden Zahlen fehlt das Rechenzeichen (·, :, + oder −), z. B. 2/3 + 1/4."),
            Self::SlashBetweenFractions => f.write_str("Zum Teilen bitte : verwenden – der Schrägstrich / ist der Bruchstrich."),
            Self::TooManyOperands => {
                f.write_str("Bitte genau zwei Zahlen mit einem Rechenzeichen eingeben, z. B. 2/3 · 3/4. Längere Rechnungen gehen im Tab «Gemischte Rechnungen».")
            }
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
        Mode::Reduce => fractions::reduce_steps(input, fractions::Style::Numbers),
        Mode::ReduceVariables => fractions::reduce_steps(input, fractions::Style::Variables),
        Mode::ReducePowers => fractions::reduce_steps(input, fractions::Style::Powers),
        Mode::Extend => fractions::extend_steps(input, false),
        Mode::ExtendVariables => fractions::extend_steps(input, true),
        Mode::Multiply | Mode::Divide | Mode::Add | Mode::Subtract => fractions::arithmetic_steps(input),
        Mode::Combined => fractions::combined_steps(input),
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
const REDUCE_EXAMPLES: &[&str] = &["12/18", "24/36", "-14/21", "45/60", "84/126", "36/12", "13/20"];
const EXTEND_EXAMPLES: &[&str] = &["3/4 mit 5", "2/3 auf 12", "5/8 mit 3", "7/9 auf 45", "-1/6 auf 24"];
const REDUCE_VARIABLES_EXAMPLES: &[&str] = &[
    "6xy/9x",
    "12a²b/18ab³",
    "(4x + 8)/(6x + 12)",
    "(x² - 9)/(x + 3)",
    "(a² + 2ab + b²)/(3a + 3b)",
    "(2x - 6)/(9 - 3x)",
    "(x + 2)/2",
];
const EXTEND_VARIABLES_EXAMPLES: &[&str] =
    &["2a/3b mit 4a", "x/y auf xy²", "3/(4x) auf 12x²y", "x/(x + 1) mit (x - 1)", "1/(x - 2) auf x² - 4"];
const REDUCE_POWERS_EXAMPLES: &[&str] =
    &["x⁵/x³", "a⁴b²/(a²b⁵)", "15x⁷y³/(25x⁴y⁵)", "-12m³n⁶/(18m³n²)", "(x + 1)⁵/(x + 1)²", "8a²b/(12a⁴b)"];
const MULTIPLY_EXAMPLES: &[&str] = &["2/3 · 3/4", "4/9 · 15/8", "3 · 2/5", "-5/6 · 3/10", "1 1/2 · 2/3"];
const DIVIDE_EXAMPLES: &[&str] = &["3/4 : 1/2", "2/3 : 4", "5/8 : 15/16", "-7/10 : 14/15", "2 1/4 : 3/8"];
const ADD_EXAMPLES: &[&str] = &["3/8 + 1/8", "1/4 + 2/3", "5/6 + 3/4", "2/4 + 1/6", "1 1/2 + 2/3", "-1/3 + 1/2"];
const COMBINED_EXAMPLES: &[&str] = &[
    "1/2 + 1/3 · 3/4",
    "(1/2 + 1/3) · 3/5",
    "3/4 - 1/2 : 2",
    "2/3 · (1/4 + 1/2) - 1/6",
    "(5/6 - 1/3) : (1/4 + 1/8)",
    "1 1/2 + 3/4 · 2 - 1/3",
    "-(1/2 - 1/3) · 6",
];
const SUBTRACT_EXAMPLES: &[&str] = &["5/8 - 3/8", "3/4 - 1/6", "1/3 - 3/4", "2 - 3/5", "1/2 - (-1/3)"];

/// The `index`-th example for a mode (wrapping around).
pub fn example(mode: Mode, index: usize) -> &'static str {
    let list = match mode {
        Mode::Simplify => SIMPLIFY_EXAMPLES,
        Mode::Expand => EXPAND_EXAMPLES,
        Mode::Factor => FACTOR_EXAMPLES,
        Mode::Solve => SOLVE_EXAMPLES,
        Mode::Reduce => REDUCE_EXAMPLES,
        Mode::Extend => EXTEND_EXAMPLES,
        Mode::ReduceVariables => REDUCE_VARIABLES_EXAMPLES,
        Mode::ExtendVariables => EXTEND_VARIABLES_EXAMPLES,
        Mode::ReducePowers => REDUCE_POWERS_EXAMPLES,
        Mode::Multiply => MULTIPLY_EXAMPLES,
        Mode::Divide => DIVIDE_EXAMPLES,
        Mode::Add => ADD_EXAMPLES,
        Mode::Subtract => SUBTRACT_EXAMPLES,
        Mode::Combined => COMBINED_EXAMPLES,
    };
    list[index % list.len()]
}

#[cfg(test)]
mod tests {
    use super::tree::line_text;
    use super::*;

    const MODES: [Mode; 14] = [
        Mode::Combined,
        Mode::Multiply,
        Mode::Divide,
        Mode::Add,
        Mode::Subtract,
        Mode::Simplify,
        Mode::Expand,
        Mode::Factor,
        Mode::Solve,
        Mode::Reduce,
        Mode::Extend,
        Mode::ReduceVariables,
        Mode::ExtendVariables,
        Mode::ReducePowers,
    ];

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
