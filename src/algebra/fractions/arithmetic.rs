//! Calculating with fractions of numbers: multiplying, dividing, adding and subtracting.
//!
//! The input is two numbers — whole numbers, fractions or mixed numbers like `1 1/2` —
//! with one operator in between. Every step says what is done and why.
//! Longer calculations with several operators are handled in [`combined`].

mod combined;

pub use combined::steps as combined_steps;

use super::super::rational::{gcd, lcm};
use super::super::tree::{push_text, text_line};
use super::super::{color_name, AlgebraError, Line, Step};
use super::{equals, fraction_line, stacked};

const MAX_INPUT: usize = 200;
const MAX_DIGITS: usize = 7;
/// Multiples are listed while there are at most this many per denominator.
const MAX_MULTIPLES: i128 = 12;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Operation {
    Multiply,
    Divide,
    Add,
    Subtract,
}

impl Operation {
    fn symbol(self) -> &'static str {
        match self {
            Self::Multiply => "·",
            Self::Divide => ":",
            Self::Add => "+",
            Self::Subtract => "−",
        }
    }

    /// Multiplication and division ("Punktrechnung") come before addition and subtraction.
    fn is_product(self) -> bool {
        matches!(self, Self::Multiply | Self::Divide)
    }
}

/// A fraction with a positive denominator; the numerator carries the sign.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Frac {
    num: i128,
    den: i128,
}

impl Frac {
    fn negative(self) -> bool {
        self.num < 0
    }

    fn abs(self) -> Self {
        Self { num: self.num.abs(), ..self }
    }

    fn reduced(self) -> Self {
        let g = gcd(self.num, self.den).max(1);
        Self { num: self.num / g, den: self.den / g }
    }

    fn text(self) -> String {
        format!("{}/{}", signed(self.num), self.den)
    }

    fn line(self) -> Line {
        self.colored(None)
    }

    fn colored(self, color: Option<usize>) -> Line {
        let piece = |n: i128| {
            let mut line = Line::new();
            push_text(&mut line, n.to_string(), color, false);
            line
        };
        fraction_line(self.negative(), piece(self.num.abs()), piece(self.den))
    }

    /// Like [`Frac::line`], but a whole number without fraction bar.
    fn result_line(self) -> Line {
        if self.den == 1 { text_line(signed(self.num)) } else { self.line() }
    }
}

fn signed(n: i128) -> String {
    if n < 0 { format!("−{}", -n) } else { n.to_string() }
}

/// How a number was entered (without its sign).
#[derive(Debug, Clone, Copy)]
enum Written {
    Integer(i128),
    Fraction(i128, i128),
    /// Whole part, numerator, denominator.
    Mixed(i128, i128, i128),
}

#[derive(Debug, Clone, Copy)]
struct Operand {
    negative: bool,
    written: Written,
}

impl Operand {
    fn value(self) -> Frac {
        let (num, den) = match self.written {
            Written::Integer(n) => (n, 1),
            Written::Fraction(n, d) => (n, d),
            Written::Mixed(w, n, d) => (w * d + n, d),
        };
        Frac { num: if self.negative { -num } else { num }, den }
    }

    fn line(self) -> Line {
        self.colored(None)
    }

    fn colored(self, color: Option<usize>) -> Line {
        let number = |n: i128| {
            let mut line = Line::new();
            push_text(&mut line, n.to_string(), color, false);
            line
        };
        let mut line = Line::new();
        if self.negative {
            push_text(&mut line, "−", color, false);
        }
        match self.written {
            Written::Integer(n) => line.extend(number(n)),
            Written::Fraction(n, d) => line.push(stacked(number(n), number(d))),
            Written::Mixed(w, n, d) => {
                line.extend(number(w));
                line.push(stacked(number(n), number(d)));
            }
        }
        line
    }
}

/// `left op right`, with a negative right operand in brackets.
fn expression(left: Line, op: Operation, right: Line, bracket: bool) -> Line {
    let mut line = left;
    push_text(&mut line, format!(" {} ", op.symbol()), None, false);
    if bracket {
        push_text(&mut line, "(", None, false);
        line.extend(right);
        push_text(&mut line, ")", None, false);
    } else {
        line.extend(right);
    }
    line
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tok {
    /// A number, and whether whitespace came before it (for mixed numbers).
    Num(i128, bool),
    Bar,
    Plus,
    Minus,
    Times,
    Colon,
    Open,
    Close,
}

fn lex(input: &str) -> Result<Vec<Tok>, AlgebraError> {
    if input.chars().count() > MAX_INPUT {
        return Err(AlgebraError::TooLong);
    }
    let mut tokens = Vec::new();
    let mut spaced = false;
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        let token = match c {
            _ if c.is_whitespace() => {
                spaced = true;
                continue;
            }
            '0'..='9' => {
                let mut digits = String::from(c);
                while let Some(&d) = chars.peek().filter(|d| d.is_ascii_digit()) {
                    digits.push(d);
                    chars.next();
                }
                if matches!(chars.peek(), Some('.' | ',')) {
                    return Err(AlgebraError::NotInteger);
                }
                if digits.len() > MAX_DIGITS {
                    return Err(AlgebraError::NumberTooLarge);
                }
                Tok::Num(digits.parse().map_err(|_| AlgebraError::NumberTooLarge)?, spaced)
            }
            '/' | '⁄' => Tok::Bar,
            ':' | '÷' => Tok::Colon,
            '*' | '·' | '×' | '⋅' | '•' => Tok::Times,
            '+' => Tok::Plus,
            '-' | '−' | '–' => Tok::Minus,
            '(' | '[' => Tok::Open,
            ')' | ']' => Tok::Close,
            _ if c.is_alphabetic() => return Err(AlgebraError::VariablesNotAllowed),
            _ => return Err(AlgebraError::UnexpectedChar(c)),
        };
        spaced = false;
        tokens.push(token);
    }
    Ok(tokens)
}

struct Parser {
    tokens: Vec<Tok>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> Option<Tok> {
        self.tokens.get(self.pos).copied()
    }

    fn next(&mut self) -> Option<Tok> {
        let token = self.peek();
        self.pos += usize::from(token.is_some());
        token
    }

    fn denominator(&mut self) -> Result<i128, AlgebraError> {
        match self.next() {
            Some(Tok::Num(0, _)) => Err(AlgebraError::DivisionByZero),
            Some(Tok::Num(d, _)) => Ok(d),
            Some(_) => Err(AlgebraError::MissingOperand),
            None => Err(AlgebraError::UnexpectedEnd),
        }
    }

    /// `(…)`, a signed whole number, a fraction `a/b` or a mixed number `w a/b`.
    fn operand(&mut self) -> Result<Operand, AlgebraError> {
        match self.next() {
            Some(Tok::Open) => {
                let inner = self.operand()?;
                match self.next() {
                    Some(Tok::Close) => Ok(inner),
                    _ => Err(AlgebraError::MissingClose),
                }
            }
            Some(Tok::Minus) => {
                let inner = self.operand()?;
                Ok(Operand { negative: !inner.negative, ..inner })
            }
            Some(Tok::Plus) => self.operand(),
            Some(Tok::Num(n, _)) => {
                let written = match (self.peek(), self.tokens.get(self.pos + 1)) {
                    (Some(Tok::Bar), _) => {
                        self.pos += 1;
                        Written::Fraction(n, self.denominator()?)
                    }
                    (Some(Tok::Num(numerator, true)), Some(Tok::Bar)) => {
                        self.pos += 2;
                        Written::Mixed(n, numerator, self.denominator()?)
                    }
                    _ => Written::Integer(n),
                };
                Ok(Operand { negative: false, written })
            }
            Some(Tok::Close) => Err(AlgebraError::UnexpectedClose),
            Some(_) => Err(AlgebraError::MissingOperand),
            None => Err(AlgebraError::UnexpectedEnd),
        }
    }
}

fn parse(input: &str) -> Result<(Operand, Operation, Operand), AlgebraError> {
    let tokens = lex(input)?;
    if tokens.is_empty() {
        return Err(AlgebraError::Empty);
    }
    let mut parser = Parser { tokens, pos: 0 };
    let left = parser.operand()?;
    let op = match parser.next() {
        Some(Tok::Times) => Operation::Multiply,
        Some(Tok::Colon) => Operation::Divide,
        Some(Tok::Plus) => Operation::Add,
        Some(Tok::Minus) => Operation::Subtract,
        Some(Tok::Bar) => return Err(AlgebraError::SlashBetweenFractions),
        Some(Tok::Close) => return Err(AlgebraError::UnexpectedClose),
        _ => return Err(AlgebraError::MissingOperator),
    };
    let right = parser.operand()?;
    match parser.peek() {
        None => Ok((left, op, right)),
        Some(Tok::Bar) => Err(AlgebraError::SlashBetweenFractions),
        Some(Tok::Close) => Err(AlgebraError::UnexpectedClose),
        Some(_) => Err(AlgebraError::TooManyOperands),
    }
}

pub fn steps(input: &str) -> Result<Vec<Step>, AlgebraError> {
    let (left, op, right) = parse(input)?;
    let (a, b) = (left.value(), right.value());
    let original = expression(left.line(), op, right.line(), right.negative);
    let mut steps = Vec::new();
    steps.extend(conversion_step(left, op, right));
    let result = match op {
        Operation::Multiply | Operation::Divide => product_steps(a, op, b, &mut steps)?,
        Operation::Add | Operation::Subtract => sum_steps(a, op, b, &mut steps),
    };
    steps.push(result_step(original, result));
    Ok(steps)
}

/// Whole and mixed numbers become fractions.
fn conversion_step(left: Operand, op: Operation, right: Operand) -> Option<Step> {
    let notes: Vec<String> = [left, right]
        .iter()
        .filter_map(|operand| match operand.written {
            Written::Integer(n) => Some(format!("Eine ganze Zahl ist ein Bruch mit dem Nenner 1: {n} = {n}/1")),
            Written::Mixed(w, n, d) => Some(format!(
                "Gemischte Zahl in einen unechten Bruch umwandeln – Ganze mal Nenner plus Zähler: \
                 {w} {n}/{d} = ({w}·{d} + {n})/{d} = {}/{d}",
                w * d + n
            )),
            Written::Fraction(..) => None,
        })
        .collect();
    if notes.is_empty() {
        return None;
    }
    let why = match op {
        Operation::Multiply | Operation::Divide => "Warum? Zum Multiplizieren und Dividieren braucht jede Zahl einen Zähler und einen Nenner.",
        Operation::Add | Operation::Subtract => "Warum? So haben beide Zahlen die Form Zähler/Nenner, und wir können sie auf einen gemeinsamen Nenner bringen.",
    };
    let (a, b) = (left.value(), right.value());
    Some(Step::new(
        "Als Brüche schreiben",
        format!("{}\n{why}", notes.join("\n")),
        expression(a.line(), op, b.line(), b.negative()),
    ))
}

/// Multiplication (division via the reciprocal): sign, one fraction bar, cancel, multiply.
fn product_steps(a: Frac, op: Operation, b: Frac, steps: &mut Vec<Step>) -> Result<Frac, AlgebraError> {
    let b = if op == Operation::Divide {
        if b.num == 0 {
            return Err(AlgebraError::DivisionByZero);
        }
        let flipped = Frac { num: b.num.signum() * b.den, den: b.num.abs() };
        steps.push(reciprocal_step(a, b, flipped));
        flipped
    } else {
        b
    };
    let times = Operation::Multiply;
    if a.num == 0 || b.num == 0 {
        steps.push(Step::new(
            "Mal 0",
            "Ist ein Faktor 0, ist das ganze Produkt 0. Warum? Nimmt man von 0 einen Teil (oder 0-mal etwas), bleibt nichts.",
            equals(expression(a.line(), times, b.line(), b.negative()), text_line("0")),
        ));
        return Ok(Frac { num: 0, den: 1 });
    }

    let negative = a.negative() != b.negative();
    if a.negative() || b.negative() {
        let mut line = fraction_sign(negative);
        line.extend(expression(a.abs().line(), times, b.abs().line(), false));
        let explanation = match (a.negative(), b.negative()) {
            (true, true) => "Beide Brüche sind negativ. Minus mal Minus ergibt Plus – das Ergebnis ist positiv.",
            _ => "Einer der Brüche ist negativ. Plus mal Minus ergibt Minus – das Minus kommt vor das Ergebnis.",
        };
        steps.push(Step::new(
            "Vorzeichen bestimmen",
            format!("{explanation}\nWarum zuerst? Danach rechnen wir nur noch mit positiven Zahlen."),
            line,
        ));
    }

    let (tops, bottoms) = ([a.num.abs(), b.num.abs()], [a.den, b.den]);
    steps.push(Step::new(
        "Auf einen Bruchstrich schreiben",
        "Brüche multipliziert man, indem man Zähler mal Zähler und Nenner mal Nenner rechnet.\n\
         Warum? Ein Bruch von einem Bruch ist ein Teil von einem Teil: Die Hälfte von einer Hälfte ist ein Viertel, \
         also ½ · ½ = (1·1)/(2·2) = ¼.",
        fraction_line(negative, product_line(&tops, &[None; 2], &[false; 2]), product_line(&bottoms, &[None; 2], &[false; 2])),
    ));

    // Cancel every numerator against every denominator, also crosswise.
    let (mut top, mut bottom) = (tops, bottoms);
    let mut top_marks = [None; 2];
    let mut bottom_marks = [None; 2];
    let mut lines = Vec::new();
    for (i, j) in [(0, 1), (1, 0), (0, 0), (1, 1)] {
        let g = gcd(top[i], bottom[j]);
        if g > 1 {
            let color = lines.len();
            top_marks[i].get_or_insert(color);
            bottom_marks[j].get_or_insert(color);
            lines.push(format!(
                "{} und {} haben den gemeinsamen Teiler {g} ({}): {} : {g} = {}, {} : {g} = {}",
                top[i],
                bottom[j],
                color_name(color),
                top[i],
                top[i] / g,
                bottom[j],
                bottom[j] / g
            ));
            top[i] /= g;
            bottom[j] /= g;
        }
    }
    if !lines.is_empty() {
        let struck_top = top_marks.map(|m| m.is_some());
        let struck_bottom = bottom_marks.map(|m| m.is_some());
        let before = fraction_line(negative, product_line(&tops, &top_marks, &struck_top), product_line(&bottoms, &bottom_marks, &struck_bottom));
        let after = fraction_line(negative, product_line(&top, &[None; 2], &[false; 2]), product_line(&bottom, &[None; 2], &[false; 2]));
        steps.push(Step::new(
            "Vor dem Ausrechnen kürzen",
            format!(
                "Ein Zähler und ein Nenner mit gemeinsamem Teiler werden beide durch diesen Teiler geteilt – auch über Kreuz:\n{}\n\
                 Warum? So bleiben die Zahlen klein, und das Ergebnis ist am Ende schon vollständig gekürzt.",
                lines.join("\n")
            ),
            equals(before, after),
        ));
    }

    let result = Frac { num: if negative { -top[0] * top[1] } else { top[0] * top[1] }, den: bottom[0] * bottom[1] };
    let after = fraction_line(negative, product_line(&top, &[None; 2], &[false; 2]), product_line(&bottom, &[None; 2], &[false; 2]));
    steps.push(Step::new(
        "Ausrechnen",
        format!("Zähler: {}·{} = {}\nNenner: {}·{} = {}", top[0], top[1], top[0] * top[1], bottom[0], bottom[1], result.den),
        equals(after, result.result_line()),
    ));
    Ok(result)
}

fn reciprocal_step(a: Frac, b: Frac, flipped: Frac) -> Step {
    let line = expression(a.line(), Operation::Multiply, flipped.colored(Some(0)), flipped.negative());
    let what = if b.den == 1 {
        format!("Eine ganze Zahl hat als Kehrwert 1 durch die Zahl: {} → {} ({}).", signed(b.num), flipped.text(), color_name(0))
    } else {
        format!("Beim Kehrwert tauschen Zähler und Nenner die Plätze: {} → {} ({}).", b.text(), flipped.text(), color_name(0))
    };
    let step = Step::new(
        "Mit dem Kehrwert multiplizieren",
        format!(
            "Durch einen Bruch teilt man, indem man mit seinem Kehrwert multipliziert. {what}\n\
             Warum? Teilen fragt: Wie oft passt der Teiler hinein? In 1 Ganzes passt ½ zweimal – \
             durch ½ teilen ist also dasselbe wie mal 2."
        ),
        line,
    );
    match b.negative() {
        true => step.with_hint(format!("Das Minus bleibt beim Kehrwert erhalten: Der Kehrwert von {} ist {}.", b.text(), flipped.text())),
        false => step,
    }
}

/// A product of numbers like `2·9`, with marker colours and struck factors.
fn product_line(values: &[i128; 2], colors: &[Option<usize>; 2], struck: &[bool; 2]) -> Line {
    let mut line = Line::new();
    for (k, ((value, color), struck)) in values.iter().zip(colors).zip(struck).enumerate() {
        if k > 0 {
            push_text(&mut line, "·", None, false);
        }
        push_text(&mut line, value.to_string(), *color, false);
        if let Some(token) = line.last_mut() {
            token.struck = *struck;
        }
    }
    line
}

/// The German name of the pieces a denominator cuts into ("Drittel"), if there is a common one.
fn piece_name(den: i128) -> String {
    const NAMES: [&str; 12] =
        ["Ganze", "Halbe", "Drittel", "Viertel", "Fünftel", "Sechstel", "Siebtel", "Achtel", "Neuntel", "Zehntel", "Elftel", "Zwölftel"];
    usize::try_from(den - 1).ok().and_then(|i| NAMES.get(i)).map_or_else(|| format!("{den}-tel"), |name| name.to_string())
}

/// `count` pieces of size 1/den, e.g. "3 Zwölftel".
fn pieces(count: i128, den: i128) -> String {
    format!("{} {}", signed(count), piece_name(den))
}

/// A highlighted minus sign in front of the fraction, if the result is negative.
fn fraction_sign(negative: bool) -> Line {
    let mut line = Line::new();
    if negative {
        push_text(&mut line, "−", None, true);
    }
    line
}

/// Addition and subtraction: signs, reduce, common denominator, expand, combine numerators.
fn sum_steps(a: Frac, op: Operation, b: Frac, steps: &mut Vec<Step>) -> Frac {
    let (op, b) = if b.negative() {
        let flipped = if op == Operation::Add { Operation::Subtract } else { Operation::Add };
        let mut line = a.line();
        push_text(&mut line, format!(" {} ", flipped.symbol()), None, true);
        line.extend(b.abs().line());
        let explanation = match op {
            Operation::Subtract => "Minus mal Minus ergibt Plus: Eine negative Zahl abziehen ist dasselbe, wie sie zu addieren.",
            _ => "Plus mal Minus ergibt Minus: Eine negative Zahl addieren ist dasselbe, wie sie abzuziehen.",
        };
        steps.push(Step::new(
            "Vorzeichen zusammenfassen",
            format!("{explanation}\nWarum zuerst? Dann steht nur noch ein Rechenzeichen zwischen den Brüchen."),
            line,
        ));
        (flipped, b.abs())
    } else {
        (op, b)
    };

    let (ra, rb) = (a.reduced(), b.reduced());
    if ra != a || rb != b {
        let notes: Vec<String> = [(a, ra), (b, rb)]
            .iter()
            .filter(|(before, after)| before != after)
            .map(|(before, after)| format!("{} = {} (Zähler und Nenner durch {} geteilt)", before.text(), after.text(), before.den / after.den))
            .collect();
        steps.push(Step::new(
            "Zuerst kürzen",
            format!(
                "{}\nWarum? Mit gekürzten Brüchen wird der gemeinsame Nenner kleiner und das Rechnen einfacher.",
                notes.join("\n")
            ),
            expression(ra.line(), op, rb.line(), false),
        ));
    }
    let (a, b) = (ra, rb);

    let common = lcm(a.den, b.den);
    let (ka, kb) = (common / a.den, common / b.den);
    let verb = if op == Operation::Add { "addieren" } else { "subtrahieren" };
    if a.den != b.den {
        let mut lines = vec![
            format!(
                "Brüche kann man nur {verb}, wenn sie den gleichen Nenner haben – also gleich grosse Stücke. \
                 {} und {} lassen sich nicht direkt verrechnen.",
                piece_name(a.den),
                piece_name(b.den)
            ),
            "Der kleinste gemeinsame Nenner (Hauptnenner) ist das kgV der beiden Nenner:".to_string(),
        ];
        if ka <= MAX_MULTIPLES && kb <= MAX_MULTIPLES {
            for den in [a.den, b.den] {
                let multiples: Vec<String> = (1..=common / den).map(|k| (k * den).to_string()).collect();
                lines.push(format!("Vielfache von {den}: {}", multiples.join(", ")));
            }
        }
        lines.push(format!("Die kleinste Zahl in beiden Listen: kgV({}, {}) = {common}", a.den, b.den));
        steps.push(Step::new("Hauptnenner bestimmen", lines.join("\n"), text_line(format!("kgV({}, {}) = {common}", a.den, b.den))));

        let expand = |f: Frac, k: i128, color: usize| {
            let part = |n: i128| {
                let mut line = text_line(n.to_string());
                if k > 1 {
                    push_text(&mut line, "·", None, false);
                    push_text(&mut line, k.to_string(), Some(color), false);
                }
                line
            };
            fraction_line(f.negative(), part(f.num.abs()), part(f.den))
        };
        // `which` is the ordinal in nominative and accusative ("der erste", "den ersten").
        let note = |which: (&str, &str), f: Frac, k: i128, color: usize| match k {
            1 => format!("Der {} Bruch hat schon den Nenner {common} und bleibt, wie er ist.", which.0),
            _ => format!("{common} : {} = {k} → den {} Bruch mit {k} erweitern ({})", f.den, which.1, color_name(color)),
        };
        let expanded = (Frac { num: a.num * ka, den: common }, Frac { num: b.num * kb, den: common });
        steps.push(Step::new(
            "Auf den Hauptnenner erweitern",
            format!(
                "{}\n{}\nWarum? Erweitern ändert den Wert eines Bruchs nicht – die Stücke werden nur feiner eingeteilt.",
                note(("erste", "ersten"), a, ka, 0),
                note(("zweite", "zweiten"), b, kb, 1)
            ),
            equals(
                expression(expand(a, ka, 0), op, expand(b, kb, 1), false),
                expression(expanded.0.line(), op, expanded.1.line(), false),
            ),
        ));
    }

    let (x, y) = (a.num * ka, b.num * kb);
    let r = if op == Operation::Add { x + y } else { x - y };
    let sum = Frac { num: r, den: common };
    let numerator = text_line(format!("{} {} {y}", signed(x), op.symbol()));
    let mut result = sum.line();
    if r < 0 && x >= 0 {
        result[0].sign = true;
    }
    let word = if op == Operation::Add { "plus" } else { "weniger" };
    let step = Step::new(
        format!("Zähler {verb}"),
        format!(
            "Jetzt sind die Nenner gleich. Man rechnet nur mit den Zählern, der Nenner {common} bleibt.\n\
             Warum? Der Nenner sagt, wie gross die Stücke sind: {} {word} {} ergeben {}.",
            pieces(x, common),
            pieces(y, common),
            pieces(r, common)
        ),
        equals(fraction_line(false, numerator, text_line(common.to_string())), result),
    );
    let calculation = format!("{} {} {y}", signed(x), op.symbol());
    let hint = match (x < 0, r < 0) {
        (false, true) => format!("{calculation}: Es wird mehr abgezogen, als vorhanden ist – darum ist das Ergebnis negativ."),
        (true, false) if r != 0 => format!("{calculation}: Es wird mehr dazugezählt als abgezogen – darum ist das Ergebnis positiv."),
        _ => String::new(),
    };
    steps.push(step.with_hint(hint));

    let reduced = sum.reduced();
    let g = sum.den / reduced.den;
    if r != 0 && g > 1 {
        steps.push(Step::new(
            "Ergebnis kürzen",
            format!(
                "Zähler und Nenner haben den gemeinsamen Teiler {g}: {} : {g} = {}, {common} : {g} = {}.\n\
                 Warum? Ein Ergebnis gibt man immer vollständig gekürzt an.",
                r.abs(),
                reduced.num.abs(),
                reduced.den
            ),
            equals(sum.line(), reduced.result_line()),
        ));
    }
    reduced
}

fn result_step(original: Line, result: Frac) -> Step {
    let explanation = match result.den {
        _ if result.num == 0 => "Das Ergebnis ist 0.",
        1 => "Das Ergebnis ist eine ganze Zahl.",
        _ => "Das Ergebnis ist vollständig gekürzt.",
    };
    let step = Step::new("Ergebnis", explanation, equals(original, result.result_line()));
    let (n, d) = (result.num.abs(), result.den);
    if d > 1 && n > d {
        let sign = if result.negative() { "−" } else { "" };
        step.with_hint(format!("Der Zähler ist grösser als der Nenner (unechter Bruch). Als gemischte Zahl: {sign}{} {}/{d}.", n / d, n % d))
    } else {
        step
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::tree::line_text;
    use super::*;

    fn result(input: &str) -> String {
        line_text(&steps(input).unwrap().last().unwrap().line)
    }

    fn titles(input: &str) -> Vec<String> {
        steps(input).unwrap().into_iter().map(|s| s.title).collect()
    }

    #[test]
    fn multiplies_with_cancelling_first() {
        assert_eq!(result("2/3 · 3/4"), "2/3 · 3/4 = 1/2");
        assert_eq!(result("4/9 * 15/8"), "4/9 · 15/8 = 5/6");
        assert_eq!(result("3 · 2/5"), "3 · 2/5 = 6/5");
        assert_eq!(result("-5/6 · 3/10"), "−5/6 · 3/10 = −1/4");
        assert_eq!(result("1 1/2 · 2/3"), "11/2 · 2/3 = 1");
        assert_eq!(result("0 · 3/4"), "0 · 3/4 = 0");
        assert_eq!(
            titles("2/3 · 3/4"),
            ["Auf einen Bruchstrich schreiben", "Vor dem Ausrechnen kürzen", "Ausrechnen", "Ergebnis"]
        );
        assert_eq!(titles("1/2 · 1/3")[1], "Ausrechnen");
    }

    #[test]
    fn divides_by_multiplying_with_the_reciprocal() {
        assert_eq!(result("3/4 : 1/2"), "3/4 : 1/2 = 3/2");
        assert_eq!(result("2/3 : 4"), "2/3 : 4 = 1/6");
        assert_eq!(result("-7/10 : 14/15"), "−7/10 : 14/15 = −3/4");
        assert_eq!(result("1/2 : (-1/4)"), "1/2 : (−1/4) = −2");
        let steps = steps("3/4 : 1/2").unwrap();
        assert_eq!(steps[0].title, "Mit dem Kehrwert multiplizieren");
        assert_eq!(line_text(&steps[0].line), "3/4 · 2/1");
        assert_eq!(super::steps("3/4 : 0").unwrap_err(), AlgebraError::DivisionByZero);
    }

    #[test]
    fn adds_and_subtracts_on_a_common_denominator() {
        assert_eq!(result("1/4 + 2/3"), "1/4 + 2/3 = 11/12");
        assert_eq!(result("3/8 + 1/8"), "3/8 + 1/8 = 1/2");
        assert_eq!(result("5/6 + 3/4"), "5/6 + 3/4 = 19/12");
        assert_eq!(result("1/3 - 3/4"), "1/3 − 3/4 = −5/12");
        assert_eq!(result("2 - 3/5"), "2 − 3/5 = 7/5");
        assert_eq!(result("1/2 - (-1/3)"), "1/2 − (−1/3) = 5/6");
        assert_eq!(result("2/4 + 1/6"), "2/4 + 1/6 = 2/3");
        assert_eq!(result("1/2 - 1/2"), "1/2 − 1/2 = 0");
        assert_eq!(
            titles("1/4 + 2/3"),
            ["Hauptnenner bestimmen", "Auf den Hauptnenner erweitern", "Zähler addieren", "Ergebnis"]
        );
        assert_eq!(titles("3/8 + 1/8"), ["Zähler addieren", "Ergebnis kürzen", "Ergebnis"]);
        assert_eq!(titles("2/4 + 1/6")[0], "Zuerst kürzen");
        let negative = steps("1/3 - 3/4").unwrap();
        assert!(negative.iter().any(|s| s.hint.contains("mehr abgezogen")));
        assert!(super::steps("5/6 + 3/4").unwrap().last().unwrap().hint.contains("1 7/12"));
    }

    #[test]
    fn reports_input_errors() {
        assert_eq!(parse("").unwrap_err(), AlgebraError::Empty);
        assert_eq!(parse("2/3").unwrap_err(), AlgebraError::MissingOperator);
        assert_eq!(parse("2/3 3/4").unwrap_err(), AlgebraError::MissingOperator);
        assert_eq!(parse("2/3/4/5").unwrap_err(), AlgebraError::SlashBetweenFractions);
        assert_eq!(parse("1/2 + 1/3 + 1/4").unwrap_err(), AlgebraError::TooManyOperands);
        assert_eq!(parse("x/2 + 1").unwrap_err(), AlgebraError::VariablesNotAllowed);
        assert_eq!(parse("1,5 + 1/2").unwrap_err(), AlgebraError::NotInteger);
        assert_eq!(parse("1/0 + 1/2").unwrap_err(), AlgebraError::DivisionByZero);
        assert_eq!(parse("(1/2 + 1/3").unwrap_err(), AlgebraError::MissingClose);
    }
}
