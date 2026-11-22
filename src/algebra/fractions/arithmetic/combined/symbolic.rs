//! Calculating with fractions that contain variables, like `2a/3b · 9b/4a` or `1/x + 1/y`.
//!
//! Numerators and denominators are kept in factored form ([`Part`]): products are
//! cancelled factor by factor, and the common denominator is built from the factors
//! of both denominators. Sums are only multiplied out where they are added.

use super::super::super::super::parse;
use super::super::super::super::rational::{gcd, lcm};
use super::super::super::super::tree::{
    line_text, push_text, text_line, Factor, Group, Mono, Sum, Summand, Vars,
};
use super::super::super::super::{color_name, AlgebraError, Glyph, Line, Step};
use super::super::super::{
    equals, fraction_line, items_line, normalized, unparen, written as written_line, Atom,
    Factored, Item, Part, Poly, LIMIT,
};
use super::super::{fraction_sign, Operation};
use super::{checked, group, negate, starts_with_minus, Expr, Value, MAX_DEPTH};

const MAX_INPUT: usize = 200;
const MAX_DIGITS: usize = 7;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// A number, a variable or an exponent: parts of a numerator or denominator.
    Atom,
    Bar,
    Colon,
    Times,
    Plus,
    Minus,
    Open,
    Close,
}

/// A token with its position in the input, so that numerators and denominators can be
/// handed to the term parser as they were written.
#[derive(Debug, Clone, Copy)]
struct Tok {
    kind: Kind,
    start: usize,
    end: usize,
}

fn lex(input: &str) -> Result<Vec<Tok>, AlgebraError> {
    if input.chars().count() > MAX_INPUT {
        return Err(AlgebraError::TooLong);
    }
    let mut tokens = Vec::new();
    let mut chars = input.char_indices().peekable();
    while let Some((start, c)) = chars.next() {
        let kind = match c {
            _ if c.is_whitespace() => continue,
            '0'..='9' => {
                let mut digits = 1;
                while chars.next_if(|(_, d)| d.is_ascii_digit()).is_some() {
                    digits += 1;
                }
                if matches!(chars.peek(), Some((_, '.' | ','))) {
                    return Err(AlgebraError::NotInteger);
                }
                if digits > MAX_DIGITS {
                    return Err(AlgebraError::NumberTooLarge);
                }
                Kind::Atom
            }
            _ if c.is_ascii_alphabetic() || c == '^' || "⁰¹²³⁴⁵⁶⁷⁸⁹".contains(c) => {
                Kind::Atom
            }
            '/' | '⁄' => Kind::Bar,
            ':' | '÷' => Kind::Colon,
            '*' | '·' | '×' | '⋅' | '•' => Kind::Times,
            '+' => Kind::Plus,
            '-' | '−' | '–' => Kind::Minus,
            '(' | '[' | '{' => Kind::Open,
            ')' | ']' | '}' => Kind::Close,
            _ => return Err(AlgebraError::UnexpectedChar(c)),
        };
        let end = chars.peek().map_or(input.len(), |&(i, _)| i);
        tokens.push(Tok { kind, start, end });
    }
    Ok(tokens)
}

/// For each opening bracket: its closing bracket, and whether it holds a calculation
/// (a fraction bar or `:` inside) rather than a sum like `(x + 1)`.
fn brackets(tokens: &[Tok]) -> Result<Vec<Option<(usize, bool)>>, AlgebraError> {
    let mut info = vec![None; tokens.len()];
    let mut open: Vec<(usize, bool)> = Vec::new();
    for (i, token) in tokens.iter().enumerate() {
        match token.kind {
            Kind::Open => open.push((i, false)),
            Kind::Close => {
                let (start, calculation) = open.pop().ok_or(AlgebraError::UnexpectedClose)?;
                info[start] = Some((i, calculation));
                if let Some(outer) = open.last_mut() {
                    outer.1 |= calculation;
                }
            }
            Kind::Bar | Kind::Colon => {
                if let Some(inner) = open.last_mut() {
                    inner.1 = true;
                }
            }
            _ => {}
        }
    }
    if open.is_empty() {
        Ok(info)
    } else {
        Err(AlgebraError::MissingClose)
    }
}

struct Parser<'a> {
    input: &'a str,
    tokens: Vec<Tok>,
    brackets: Vec<Option<(usize, bool)>>,
    pos: usize,
}

impl<'a> Parser<'a> {
    fn kind(&self) -> Option<Kind> {
        self.tokens.get(self.pos).map(|t| t.kind)
    }

    /// The token at `pos` opens a bracket: its closing position and whether it holds a calculation.
    fn bracket(&self) -> Option<(usize, bool)> {
        self.brackets.get(self.pos).copied().flatten()
    }

    fn sum(&mut self, depth: usize) -> Result<Expr<Term>, AlgebraError> {
        let mut expr = self.product(depth)?;
        loop {
            let op = match self.kind() {
                Some(Kind::Plus) => Operation::Add,
                Some(Kind::Minus) => Operation::Subtract,
                _ => return Ok(expr),
            };
            self.pos += 1;
            expr = Expr::Bin(Box::new(expr), op, Box::new(self.product(depth)?));
        }
    }

    fn product(&mut self, depth: usize) -> Result<Expr<Term>, AlgebraError> {
        let mut expr = self.factor(depth)?;
        loop {
            let op = match self.kind() {
                Some(Kind::Times) => Operation::Multiply,
                Some(Kind::Colon) => Operation::Divide,
                _ => return Ok(expr),
            };
            self.pos += 1;
            expr = Expr::Bin(Box::new(expr), op, Box::new(self.factor(depth)?));
        }
    }

    fn factor(&mut self, depth: usize) -> Result<Expr<Term>, AlgebraError> {
        match self.kind() {
            Some(Kind::Open) if self.bracket().is_some_and(|(_, calculation)| calculation) => {
                if depth >= MAX_DEPTH {
                    return Err(AlgebraError::TooDeep);
                }
                self.pos += 1;
                let inner = self.sum(depth + 1)?;
                match self.kind() {
                    Some(Kind::Close) => {
                        self.pos += 1;
                        Ok(group(inner))
                    }
                    _ => Err(AlgebraError::MissingOperator),
                }
            }
            Some(Kind::Minus) => {
                self.pos += 1;
                Ok(negate(self.factor(depth)?))
            }
            Some(Kind::Plus) => {
                self.pos += 1;
                self.factor(depth)
            }
            _ => {
                let top = self.block()?;
                let bottom = match self.kind() {
                    Some(Kind::Bar) => {
                        self.pos += 1;
                        Some(self.block()?)
                    }
                    _ => None,
                };
                Ok(Expr::Num(Term::written(top, bottom)?, false))
            }
        }
    }

    /// Numbers, variables, exponents and brackets around sums written next to each other,
    /// like `3x²(x + 1)`: one numerator or denominator.
    fn block(&mut self) -> Result<&'a str, AlgebraError> {
        let start = self.pos;
        loop {
            match (self.kind(), self.bracket()) {
                (Some(Kind::Atom), _) => self.pos += 1,
                (Some(Kind::Open), Some((close, false))) => self.pos = close + 1,
                _ => break,
            }
        }
        if self.pos == start {
            return Err(match self.kind() {
                Some(Kind::Close) => AlgebraError::UnexpectedClose,
                None => AlgebraError::UnexpectedEnd,
                Some(_) => AlgebraError::MissingOperand,
            });
        }
        Ok(&self.input[self.tokens[start].start..self.tokens[self.pos - 1].end])
    }
}

pub(super) fn parse(input: &str) -> Result<Expr<Term>, AlgebraError> {
    let tokens = lex(input)?;
    if tokens.is_empty() {
        return Err(AlgebraError::Empty);
    }
    let brackets = brackets(&tokens)?;
    let mut parser = Parser {
        input,
        tokens,
        brackets,
        pos: 0,
    };
    let expr = parser.sum(0)?;
    match parser.kind() {
        None => checked(expr),
        Some(Kind::Bar) => Err(AlgebraError::SlashBetweenFractions),
        Some(Kind::Close) => Err(AlgebraError::UnexpectedClose),
        Some(_) => Err(AlgebraError::MissingOperator),
    }
}

/// How a fraction is drawn.
#[derive(Debug, Clone)]
enum Shown {
    /// As it was entered; `sum` for a sum without fraction bar.
    Written {
        minus: bool,
        top: Line,
        bottom: Option<Line>,
        sum: bool,
    },
    /// With the numerator multiplied out, as after adding.
    Expanded(Vec<Mono>),
    /// Numerator and denominator as products.
    Factored,
}

/// A fraction of terms; a term without fraction bar has the denominator 1.
#[derive(Debug, Clone)]
pub(super) struct Term {
    /// The numerator, which also carries the sign of the fraction.
    num: Part,
    /// The denominator, always positive.
    den: Part,
    shown: Shown,
    /// How the entered numerator and denominator were factored, e.g. `4x + 8 = 4(x + 2)`.
    notes: Vec<String>,
    /// A negative number was factored out, which flips the signs in the bracket.
    flipped: bool,
}

impl Term {
    fn new(num: Part, den: Part, shown: Shown) -> Self {
        let negative = (num.negative != den.negative) && !num.is_zero();
        Self {
            num: Part { negative, ..num },
            den: den.abs(),
            shown,
            notes: Vec::new(),
            flipped: false,
        }
    }

    fn zero() -> Self {
        Self::new(
            Part {
                number: 0,
                ..Part::one()
            },
            Part::one(),
            Shown::Factored,
        )
    }

    fn written(top: &str, bottom: Option<&str>) -> Result<Self, AlgebraError> {
        let top_expr = parse::expression(top)?;
        let numerator = Factored::new(&top_expr)?;
        let is_sum = Sum::from_expr(unparen(&top_expr))?.0.len() > 1;
        let mut notes = numerator.notes;
        let mut flipped = numerator.negative_content;
        let (den, bottom_line) = match bottom {
            Some(text) => {
                let expr = parse::expression(text)?;
                let denominator = Factored::new(&expr)?;
                if denominator.part.is_zero() {
                    return Err(AlgebraError::DivisionByZero);
                }
                notes.extend(denominator.notes);
                flipped |= denominator.negative_content;
                (denominator.part, Some(written_line(&expr)?))
            }
            None => (Part::one(), None),
        };
        let shown = Shown::Written {
            minus: false,
            top: written_line(&top_expr)?,
            sum: is_sum && bottom.is_none(),
            bottom: bottom_line,
        };
        Ok(Self {
            notes,
            flipped,
            ..Self::new(numerator.part, den, shown)
        })
    }

    /// The result of an addition: kept factored, drawn multiplied out.
    fn expanded(terms: Vec<Mono>, den: Part) -> Result<Self, AlgebraError> {
        let num = Factored::from_terms(&terms)?.part;
        Ok(Self::new(num, den, Shown::Expanded(terms)))
    }

    fn factored_line(&self) -> Line {
        if self.den.is_one() {
            self.num.signed_line()
        } else {
            fraction_line(
                self.num.negative,
                self.num.abs().sum().line(),
                self.den.sum().line(),
            )
        }
    }

    /// The same fraction drawn as products.
    fn factored(&self) -> Self {
        Self {
            shown: Shown::Factored,
            notes: Vec::new(),
            flipped: false,
            ..self.clone()
        }
    }

    /// What has to be factored before this fraction can be cancelled.
    fn factoring(&self) -> (Vec<String>, bool) {
        match &self.shown {
            Shown::Written { .. } => (self.notes.clone(), self.flipped),
            Shown::Expanded(terms) => Factored::from_terms(terms)
                .map_or((Vec::new(), false), |f| (f.notes, f.negative_content)),
            Shown::Factored => (Vec::new(), false),
        }
    }

    fn abs(&self) -> Self {
        if self.num.negative {
            self.negated()
        } else {
            self.clone()
        }
    }

    /// The numerator multiplied out, with the sign of the fraction.
    fn terms(&self) -> Result<Vec<Mono>, AlgebraError> {
        match &self.shown {
            Shown::Expanded(terms) => Ok(terms.clone()),
            _ if self.num.is_zero() => Ok(Vec::new()),
            _ => self.num.expand(),
        }
    }

    fn reciprocal(&self) -> Self {
        Self::new(
            Part {
                negative: self.num.negative,
                ..self.den.clone()
            },
            self.num.abs(),
            Shown::Factored,
        )
    }

    /// The fraction cancelled as far as possible, and the factor it was divided by.
    fn reduced(&self) -> Option<(Self, Part)> {
        let common = self.num.common(&self.den);
        (!self.num.is_zero() && !common.is_one()).then(|| {
            (
                Self::new(
                    self.num.without(&common),
                    self.den.without(&common),
                    Shown::Factored,
                ),
                common,
            )
        })
    }
}

impl Value for Term {
    fn line(&self, color: Option<usize>) -> Line {
        let line = match &self.shown {
            Shown::Written {
                minus,
                top,
                bottom: Some(bottom),
                ..
            } => fraction_line(*minus, top.clone(), bottom.clone()),
            Shown::Written {
                minus,
                top,
                bottom: None,
                sum,
            } => {
                let mut line = Line::new();
                if *minus {
                    push_text(&mut line, "−", None, false);
                }
                bracketed(&mut line, top.clone(), *minus && *sum);
                line
            }
            Shown::Expanded(terms) => {
                let top = number_line(terms);
                if self.den.is_one() {
                    top
                } else {
                    fraction_line(false, top, self.den.sum().line())
                }
            }
            Shown::Factored => self.factored_line(),
        };
        match color {
            Some(color) => paint(line, color),
            None => line,
        }
    }

    fn negated(&self) -> Self {
        let mut term = self.clone();
        if term.num.is_zero() {
            return term;
        }
        term.num.negative = !term.num.negative;
        match &mut term.shown {
            Shown::Written { minus, .. } => *minus = !*minus,
            Shown::Expanded(terms) => *terms = terms.iter().map(Mono::neg).collect(),
            Shown::Factored => {}
        }
        term
    }

    fn is_sum(&self) -> bool {
        match &self.shown {
            Shown::Written {
                minus,
                bottom: None,
                sum,
                ..
            } => *sum && !*minus,
            Shown::Expanded(terms) => self.den.is_one() && terms.len() > 1,
            Shown::Factored => {
                self.den.is_one() && !self.num.negative && self.num.sum().0.len() > 1
            }
            Shown::Written { .. } => false,
        }
    }

    fn calculate(
        a: &Self,
        op: Operation,
        b: &Self,
        steps: &mut Vec<Step>,
    ) -> Result<Self, AlgebraError> {
        match op {
            Operation::Multiply | Operation::Divide => product_steps(a, op, b, steps),
            Operation::Add | Operation::Subtract => sum_steps(a, op, b, steps),
        }
    }

    fn result_step(original: Line, value: &Self) -> Step {
        let explanation = if value.num.is_zero() {
            "Das Ergebnis ist 0."
        } else {
            "Das Ergebnis ist vollständig gekürzt."
        };
        let step = Step::new("Ergebnis", explanation, equals(original, value.line(None)));
        let expanded = value.terms().map(|t| number_line(&t)).unwrap_or_default();
        if value.den.is_one() && !value.num.polys.is_empty() && line_text(&expanded) != value.text()
        {
            step.with_hint(format!(
                "Ausmultipliziert lautet das Ergebnis: {}",
                line_text(&expanded)
            ))
        } else {
            step
        }
    }
}

/// A sum of monomials, or `0` if there are none.
fn number_line(terms: &[Mono]) -> Line {
    if terms.is_empty() {
        text_line("0")
    } else {
        Sum::from_terms(terms.to_vec()).line()
    }
}

/// Gives every token of the line the marker colour.
fn paint(line: Line, color: usize) -> Line {
    line.into_iter()
        .map(|mut token| {
            token.glyph = match token.glyph {
                Glyph::Stacked(top, bottom) => {
                    Glyph::Stacked(paint(top, color), paint(bottom, color))
                }
                glyph => {
                    token.color = Some(color);
                    glyph
                }
            };
            token
        })
        .collect()
}

fn bracketed(line: &mut Line, inner: Line, bracket: bool) {
    if bracket {
        push_text(line, "(", None, false);
    }
    line.extend(inner);
    if bracket {
        push_text(line, ")", None, false);
    }
}

/// `a op b`, with brackets where a sum or a minus sign would mix with the operator.
fn pair(a: &Term, op: Operation, b: &Term) -> Line {
    styled_pair(a, op, b, false, None)
}

/// [`pair`] with a highlighted operator or a coloured second fraction.
fn styled_pair(a: &Term, op: Operation, b: &Term, sign: bool, color: Option<usize>) -> Line {
    let mut line = Line::new();
    bracketed(&mut line, a.line(None), op.is_product() && a.is_sum());
    push_text(&mut line, format!(" {} ", op.symbol()), None, sign);
    let right = b.line(color);
    let wrap =
        starts_with_minus(&right) || (b.is_sum() && (op.is_product() || op == Operation::Subtract));
    bracketed(&mut line, right, wrap);
    line
}

/// A factor as text, in brackets if it is a sum: `(x + 1)`.
fn factor_text(part: &Part) -> String {
    match part.sum().0.len() {
        1 => part.text(),
        _ => format!("({})", part.text()),
    }
}

/// "Kürzen darf man nur Faktoren": sums are factored first.
fn factoring_step(a: &Term, op: Operation, b: &Term, why: &str) -> Option<Step> {
    let ((a_notes, a_flipped), (b_notes, b_flipped)) = (a.factoring(), b.factoring());
    if a_notes.is_empty() && b_notes.is_empty() {
        return None;
    }
    let mut lines = vec![why.to_string()];
    lines.extend(a_notes.into_iter().chain(b_notes));
    let step = Step::new(
        "Faktorisieren",
        lines.join("\n"),
        pair(&a.factored(), op, &b.factored()),
    );
    Some(match a_flipped || b_flipped {
        true => step.with_hint(
            "Beim Ausklammern einer negativen Zahl kehren sich in der Klammer alle Vorzeichen um.",
        ),
        false => step,
    })
}

/// Every factor of a product on its own: number, variable powers, brackets.
fn items(part: &Part) -> Vec<Item> {
    (part.number != 1)
        .then_some(Atom::Number(part.number))
        .into_iter()
        .chain(part.vars.iter().map(|(v, e)| Atom::Var(v, e)))
        .chain(part.polys.iter().map(|(p, e)| Atom::Poly(p.clone(), *e)))
        .map(Item::new)
        .collect()
}

/// The factors above and below one fraction bar; without denominator, no bar.
fn bar_line(negative: bool, top: &[Item], bottom: &[Item]) -> Line {
    if bottom.is_empty() {
        let mut line = Line::new();
        if negative {
            push_text(&mut line, "−", None, false);
        }
        bracketed(&mut line, items_line(top), negative && top.len() > 1);
        line
    } else {
        fraction_line(negative, items_line(top), items_line(bottom))
    }
}

/// The product of the factors.
fn product_part(items: &[Item]) -> Result<Part, AlgebraError> {
    let mut part = Part::one();
    for item in items {
        match &item.atom {
            Atom::Number(n) => {
                part.number = part
                    .number
                    .checked_mul(*n)
                    .filter(|&n| n <= LIMIT)
                    .ok_or(AlgebraError::NumbersTooLarge)?;
            }
            Atom::Var(v, e) => part.vars = part.vars.mul(&Vars::single(*v, *e)),
            Atom::Poly(p, e) => part.mul_poly(p.clone(), *e),
            Atom::Mono(m) => part.mul_mono(m)?,
        }
    }
    Ok(part)
}

/// Multiplication (division via the reciprocal): factor, sign, one fraction bar, cancel, multiply.
fn product_steps(
    a: &Term,
    op: Operation,
    b: &Term,
    steps: &mut Vec<Step>,
) -> Result<Term, AlgebraError> {
    steps.extend(factoring_step(a, op, b, "Kürzen darf man nur Faktoren, keine Summanden. Darum schreiben wir Zähler und Nenner zuerst als Produkte:"));
    let (a, b) = (a.factored(), b.factored());
    let b = if op == Operation::Divide {
        if b.num.is_zero() {
            return Err(AlgebraError::DivisionByZero);
        }
        let flipped = b.reciprocal();
        steps.push(reciprocal_step(&a, &b, &flipped));
        flipped
    } else {
        b
    };
    let times = Operation::Multiply;
    if a.num.is_zero() || b.num.is_zero() {
        steps.push(Step::new(
            "Mal 0",
            "Ist ein Faktor 0, ist das ganze Produkt 0. Warum? 0-mal etwas ergibt immer 0.",
            equals(pair(&a, times, &b), text_line("0")),
        ));
        return Ok(Term::zero());
    }

    let negative = a.num.negative != b.num.negative;
    if a.num.negative || b.num.negative {
        let mut line = fraction_sign(negative);
        line.extend(pair(&a.abs(), times, &b.abs()));
        let explanation = match (a.num.negative, b.num.negative) {
            (true, true) => "Beide Faktoren sind negativ. Minus mal Minus ergibt Plus – das Ergebnis ist positiv.",
            _ => "Einer der Faktoren ist negativ. Plus mal Minus ergibt Minus – das Minus kommt vor das Ergebnis.",
        };
        steps.push(Step::new(
            "Vorzeichen bestimmen",
            format!("{explanation}\nWarum zuerst? Danach rechnen wir nur noch ohne Minuszeichen."),
            line,
        ));
    }

    let top: Vec<Item> = items(&a.num.abs())
        .into_iter()
        .chain(items(&b.num.abs()))
        .collect();
    let bottom: Vec<Item> = items(&a.den).into_iter().chain(items(&b.den)).collect();
    if !bottom.is_empty() {
        steps.push(Step::new(
            "Auf einen Bruchstrich schreiben",
            "Brüche multipliziert man, indem man Zähler mal Zähler und Nenner mal Nenner rechnet. \
             Jeder Faktor bleibt zunächst einzeln stehen.\nWarum? So sieht man, welche Faktoren oben und unten vorkommen und gekürzt werden können.",
            bar_line(negative, &top, &bottom),
        ));
    }

    let (top, bottom) = cancel_steps(negative, top, bottom, steps);
    let num = Part {
        negative,
        ..product_part(&top)?
    };
    let result = Term::new(num, product_part(&bottom)?, Shown::Factored);
    let count =
        |items: &[Item], f: &dyn Fn(&Atom) -> bool| items.iter().filter(|i| f(&i.atom)).count();
    let numbers = [&top, &bottom]
        .iter()
        .any(|side| count(side, &|a| matches!(a, Atom::Number(_))) > 1);
    let repeated = [&top, &bottom].iter().any(|side| {
        side.iter().any(|i| {
            base_of(&i.atom).is_some_and(|base| {
                count(side, &|a| base_of(a).is_some_and(|b| b.same_base(&base))) > 1
            })
        })
    });
    if numbers || repeated {
        let mut lines = Vec::new();
        if numbers {
            lines.push("Die Zahlen miteinander multiplizieren.".to_string());
        }
        if repeated {
            lines.push("Gleiche Basen multipliziert man, indem man die Hochzahlen addiert, z. B. x·x² = x³.".to_string());
        }
        lines
            .push("Variablen und Klammern schreibt man ohne Malpunkt hinter die Zahl.".to_string());
        steps.push(Step::new(
            "Zusammenfassen",
            lines.join(
                "
",
            ),
            equals(bar_line(negative, &top, &bottom), result.line(None)),
        ));
    }
    Ok(result)
}

fn reciprocal_step(a: &Term, b: &Term, flipped: &Term) -> Step {
    let what = if b.den.is_one() {
        format!(
            "Ein Term ohne Bruchstrich hat den Nenner 1. Der Kehrwert von {} ist {} ({}).",
            b.text(),
            flipped.text(),
            color_name(0)
        )
    } else {
        format!(
            "Beim Kehrwert tauschen Zähler und Nenner die Plätze: {} → {} ({}).",
            b.text(),
            flipped.text(),
            color_name(0)
        )
    };
    Step::new(
        "Mit dem Kehrwert multiplizieren",
        format!(
            "Durch einen Bruch teilt man, indem man mit seinem Kehrwert multipliziert. {what}\n\
             Warum? Ein Bruch mal sein Kehrwert ergibt 1 – Teilen durch den Bruch macht dasselbe wie Malnehmen mit dem Kehrwert."
        ),
        styled_pair(a, Operation::Multiply, flipped, false, Some(0)),
    )
}

/// The same base: equal variables or equal brackets (numbers are handled separately).
fn base_of(atom: &Atom) -> Option<Atom> {
    match atom {
        Atom::Var(v, _) => Some(Atom::Var(*v, 1)),
        Atom::Poly(p, _) => Some(Atom::Poly(p.clone(), 1)),
        _ => None,
    }
}

fn with_exponent(atom: &Atom, exponent: u32) -> Atom {
    match atom {
        Atom::Var(v, _) => Atom::Var(*v, exponent),
        Atom::Poly(p, _) => Atom::Poly(p.clone(), exponent),
        other => other.clone(),
    }
}

/// Marks and strikes the item with the colour of the first cancellation that touches it.
fn strike(item: &mut Item, color: usize) {
    item.color.get_or_insert(color);
    item.struck = true;
}

/// Removes `amount` of the base's exponent from the items, left to right.
fn take(items: &mut [Item], marked: &mut [Item], base: &Atom, mut amount: u32, color: usize) {
    for (item, mark) in items.iter_mut().zip(marked) {
        if amount == 0 {
            break;
        }
        if base_of(&item.atom).is_some_and(|b| b.same_base(base)) {
            let exponent = item.atom.exponent();
            let removed = exponent.min(amount);
            item.atom = with_exponent(&item.atom, exponent - removed);
            amount -= removed;
            strike(mark, color);
        }
    }
}

/// Cancels every factor of the numerator against the denominator, also crosswise, and
/// returns the factors that are left.
fn cancel_steps(
    negative: bool,
    top: Vec<Item>,
    bottom: Vec<Item>,
    steps: &mut Vec<Step>,
) -> (Vec<Item>, Vec<Item>) {
    let (mut marked_top, mut marked_bottom) = (top.clone(), bottom.clone());
    let (mut left_top, mut left_bottom) = (top, bottom);
    let mut lines = Vec::new();

    let pairs: Vec<(usize, usize)> = (0..left_top.len())
        .flat_map(|i| (0..left_bottom.len()).map(move |j| (i, j)))
        .collect();
    for (i, j) in pairs {
        if let (Atom::Number(a), Atom::Number(b)) = (&left_top[i].atom, &left_bottom[j].atom) {
            let (a, b) = (*a, *b);
            let g = gcd(a, b);
            if g > 1 {
                let color = lines.len();
                lines.push(format!(
                    "{a} und {b} haben den gemeinsamen Teiler {g} ({}): {a} : {g} = {}, {b} : {g} = {}",
                    color_name(color),
                    a / g,
                    b / g
                ));
                strike(&mut marked_top[i], color);
                strike(&mut marked_bottom[j], color);
                left_top[i].atom = Atom::Number(a / g);
                left_bottom[j].atom = Atom::Number(b / g);
            }
        }
    }

    let mut bases: Vec<Atom> = Vec::new();
    for base in left_top.iter().filter_map(|i| base_of(&i.atom)) {
        if !bases.iter().any(|b| b.same_base(&base)) {
            bases.push(base);
        }
    }
    for base in bases {
        let total = |items: &[Item]| -> u32 {
            items
                .iter()
                .filter(|i| base_of(&i.atom).is_some_and(|b| b.same_base(&base)))
                .map(|i| i.atom.exponent())
                .sum()
        };
        let (above, below) = (total(&left_top), total(&left_bottom));
        let common = above.min(below);
        if common == 0 {
            continue;
        }
        let color = lines.len();
        let power = |e: u32| {
            if e == 0 {
                "1".to_string()
            } else {
                base.power_text(e)
            }
        };
        lines.push(if above == common && below == common {
            format!(
                "{} steht oben und unten ({}) und fällt ganz weg.",
                base.power_text(common),
                color_name(color)
            )
        } else {
            format!(
                "{} steht oben und unten ({}): oben {} : {} = {}, unten {} : {} = {}",
                base.power_text(common),
                color_name(color),
                power(above),
                base.power_text(common),
                power(above - common),
                power(below),
                base.power_text(common),
                power(below - common)
            )
        });
        take(&mut left_top, &mut marked_top, &base, common, color);
        take(&mut left_bottom, &mut marked_bottom, &base, common, color);
    }

    let keep = |items: Vec<Item>| -> Vec<Item> {
        items
            .into_iter()
            .filter(|i| !matches!(i.atom, Atom::Number(1) | Atom::Var(_, 0) | Atom::Poly(_, 0)))
            .collect()
    };
    let (left_top, left_bottom) = (keep(left_top), keep(left_bottom));
    if !lines.is_empty() {
        let brackets = marked_top
            .iter()
            .chain(&marked_bottom)
            .any(|i| i.struck && matches!(i.atom, Atom::Poly(..)));
        let step = Step::new(
            "Vor dem Ausmultiplizieren kürzen",
            format!(
                "Faktoren, die oben und unten vorkommen, werden gestrichen – auch über Kreuz:\n{}\n\
                 Warum? Kürzen heisst, Zähler und Nenner durch denselben Faktor teilen. Der Wert bleibt gleich, und die Terme bleiben klein.",
                lines.join("\n")
            ),
            equals(bar_line(negative, &marked_top, &marked_bottom), bar_line(negative, &left_top, &left_bottom)),
        );
        steps.push(match brackets {
            true => step.with_hint(
                "Eine Klammer darf man nur als Ganzes streichen, wenn sie oben und unten als Faktor steht – nie einzelne Summanden darin.",
            ),
            false => step,
        });
    }
    (left_top, left_bottom)
}

fn same_part(a: &Part, b: &Part) -> bool {
    a.number == b.number
        && a.vars == b.vars
        && a.polys.len() == b.polys.len()
        && a.polys
            .iter()
            .all(|(p, e)| b.polys.iter().any(|(q, f)| q.same(p) && e == f))
}

/// The least common multiple: every factor of both, each with its highest exponent.
fn common_denominator(a: &Part, b: &Part) -> Result<Part, AlgebraError> {
    let number = lcm(a.number, b.number);
    if number > LIMIT {
        return Err(AlgebraError::NumbersTooLarge);
    }
    let vars = a.vars.mul(&b.vars).div(&a.vars.common(&b.vars));
    let mut polys = a.polys.clone();
    for (p, e) in &b.polys {
        match polys.iter_mut().find(|(q, _)| q.same(p)) {
            Some((_, f)) => *f = (*f).max(*e),
            None => polys.push((p.clone(), *e)),
        }
    }
    Ok(Part {
        negative: false,
        number,
        vars,
        polys,
    })
}

fn factor_of(terms: &[Mono]) -> Factor {
    match terms {
        [mono] => Factor::Mono(mono.clone()),
        _ => Factor::Group(Group {
            sum: Sum::from_terms(terms.to_vec()),
            power: 1,
        }),
    }
}

/// The numerator multiplied by the expansion factor and multiplied out.
fn expand_by(terms: &[Mono], factor: &Part) -> Result<Vec<Mono>, AlgebraError> {
    if terms.is_empty() || factor.is_one() {
        return Ok(terms.to_vec());
    }
    normalized(Sum(vec![Summand::new(vec![
        factor_of(terms),
        factor_of(&factor.expand()?),
    ])]))
}

/// Addition and subtraction: signs, factor, cancel, common denominator, expand, add numerators.
fn sum_steps(
    a: &Term,
    op: Operation,
    b: &Term,
    steps: &mut Vec<Step>,
) -> Result<Term, AlgebraError> {
    let (op, b) = if starts_with_minus(&b.line(None)) {
        let flipped = if op == Operation::Add {
            Operation::Subtract
        } else {
            Operation::Add
        };
        let b = b.negated();
        let explanation = match op {
            Operation::Subtract => "Minus mal Minus ergibt Plus: Einen negativen Term abziehen ist dasselbe, wie ihn zu addieren.",
            _ => "Plus mal Minus ergibt Minus: Einen negativen Term addieren ist dasselbe, wie ihn abzuziehen.",
        };
        steps.push(Step::new(
            "Vorzeichen zusammenfassen",
            format!("{explanation}\nWarum zuerst? Dann steht nur noch ein Rechenzeichen zwischen den Brüchen."),
            styled_pair(a, flipped, &b, true, None),
        ));
        (flipped, b)
    } else {
        (op, b.clone())
    };
    let verb = if op == Operation::Add {
        "addieren"
    } else {
        "subtrahieren"
    };

    if a.den.is_one() && b.den.is_one() {
        return polynomial_step(a, op, &b, steps);
    }
    steps.extend(factoring_step(a, op, &b, "Für den Hauptnenner brauchen wir die Nenner als Produkte – dann sieht man, welche Faktoren sie gemeinsam haben:"));
    let (a, b) = (a.factored(), b.factored());

    let (ra, rb) = (a.reduced(), b.reduced());
    let (a, b) = if ra.is_some() || rb.is_some() {
        let notes: Vec<String> = [(&a, &ra), (&b, &rb)]
            .into_iter()
            .filter_map(|(before, after)| {
                after.as_ref().map(|(after, common)| {
                    format!(
                        "{} = {} (Zähler und Nenner durch {} geteilt)",
                        before.text(),
                        after.text(),
                        factor_text(common)
                    )
                })
            })
            .collect();
        let a = ra.map_or(a, |(t, _)| t);
        let b = rb.map_or(b, |(t, _)| t);
        steps.push(Step::new(
            "Zuerst kürzen",
            format!("{}\nWarum? Mit gekürzten Brüchen wird der Hauptnenner kleiner und das Rechnen einfacher.", notes.join("\n")),
            pair(&a, op, &b),
        ));
        (a, b)
    } else {
        (a, b)
    };

    let common = if same_part(&a.den, &b.den) {
        a.den.clone()
    } else {
        common_denominator(&a.den, &b.den)?
    };
    let ka = common.divide(&a.den).ok_or(AlgebraError::Malformed)?;
    let kb = common.divide(&b.den).ok_or(AlgebraError::Malformed)?;
    let (na, nb) = (expand_by(&a.terms()?, &ka)?, expand_by(&b.terms()?, &kb)?);
    let common_line = || common.sum().line();
    if !same_part(&a.den, &b.den) {
        steps.push(common_denominator_step(&a.den, &b.den, &common, verb));

        let expand = |t: &Term, k: &Part, color: usize| -> Result<Line, AlgebraError> {
            if k.is_one() {
                return Ok(fraction_line(
                    false,
                    number_line(&t.terms()?),
                    t.den.sum().line(),
                ));
            }
            let factor: Vec<Item> = items(k).into_iter().map(|i| i.colored(color)).collect();
            let top: Vec<Item> = std::iter::once(Item::new(Atom::of_terms(&t.terms()?)))
                .chain(factor.clone())
                .collect();
            let bottom: Vec<Item> = items(&t.den).into_iter().chain(factor).collect();
            Ok(fraction_line(false, items_line(&top), items_line(&bottom)))
        };
        let note = |which: (&str, &str), t: &Term, k: &Part, color: usize| match k.is_one() {
            true => format!(
                "Der {} Bruch hat schon den Hauptnenner und bleibt, wie er ist.",
                which.0
            ),
            false => format!(
                "{} : {} = {} → den {} Bruch mit {} erweitern ({})",
                factor_text(&common),
                factor_text(&t.den),
                factor_text(k),
                which.1,
                factor_text(k),
                color_name(color)
            ),
        };
        let mut before = expand(&a, &ka, 0)?;
        push_text(&mut before, format!(" {} ", op.symbol()), None, false);
        before.extend(expand(&b, &kb, 1)?);
        let mut after = fraction_line(false, number_line(&na), common_line());
        push_text(&mut after, format!(" {} ", op.symbol()), None, false);
        after.extend(fraction_line(false, number_line(&nb), common_line()));
        steps.push(Step::new(
            "Auf den Hauptnenner erweitern",
            format!(
                "Jeder Bruch wird mit dem Faktor erweitert, der seinem Nenner zum Hauptnenner fehlt:\n{}\n{}\n\
                 Dabei den Zähler ausmultiplizieren.\nWarum? Erweitern ändert den Wert eines Bruchs nicht.",
                note(("erste", "ersten"), &a, &ka, 0),
                note(("zweite", "zweiten"), &b, &kb, 1)
            ),
            equals(before, after),
        ));
    }

    // Add or subtract the numerators.
    let wrap = nb.len() > 1 || nb.first().is_some_and(|m| m.coeff.is_negative());
    let mut numerator = number_line(&na);
    push_text(&mut numerator, format!(" {} ", op.symbol()), None, false);
    bracketed(&mut numerator, number_line(&nb), wrap);
    let signed: Vec<Mono> = if op == Operation::Subtract {
        nb.iter().map(Mono::neg).collect()
    } else {
        nb.clone()
    };
    let result = normalized(Sum::from_terms(
        na.iter().cloned().chain(signed.iter().cloned()).collect(),
    ))?;
    let mut line = fraction_line(false, numerator, common_line());
    if wrap {
        let open =
            Sum::from_terms(na.iter().cloned().chain(signed.iter().cloned()).collect()).line();
        line = equals(line, fraction_line(false, open, common_line()));
    }
    let combined = if result.is_empty() {
        text_line("0")
    } else {
        fraction_line(false, number_line(&result), common_line())
    };
    let line = if line_text(&line) == line_text(&combined) {
        line
    } else {
        equals(line, combined)
    };
    let step = Step::new(
        format!("Zähler {verb}"),
        format!(
            "Jetzt sind die Nenner gleich. Man {verb_short} nur die Zähler, der Nenner {} bleibt.\n\
             Gleichartige Terme werden zusammengefasst.\nWarum? Der Nenner gibt an, wie gross die Stücke sind – gleich grosse Stücke kann man einfach zusammenzählen.",
            factor_text(&common),
            verb_short = if op == Operation::Add { "addiert" } else { "subtrahiert" }
        ),
        line,
    );
    steps.push(match op == Operation::Subtract && wrap {
        true => step.with_hint("Minus vor einer Klammer: Beim Weglassen der Klammer drehen sich alle Vorzeichen darin um."),
        false => step,
    });
    if result.is_empty() {
        return Ok(Term::zero());
    }
    result_cancel_step(result, common, steps)
}

/// Terms without fraction bar: just combine like terms.
fn polynomial_step(
    a: &Term,
    op: Operation,
    b: &Term,
    steps: &mut Vec<Step>,
) -> Result<Term, AlgebraError> {
    let signed: Vec<Mono> = match op {
        Operation::Subtract => b.terms()?.iter().map(Mono::neg).collect(),
        _ => b.terms()?,
    };
    let result = normalized(Sum::from_terms(
        a.terms()?.into_iter().chain(signed).collect(),
    ))?;
    let step = Step::new(
        "Zusammenfassen",
        "Hier steht kein Bruchstrich. Gleichartige Terme werden zusammengefasst: gleiche Variablen mit gleicher Hochzahl.",
        equals(pair(a, op, b), number_line(&result)),
    );
    steps.push(match op == Operation::Subtract && b.is_sum() {
        true => step.with_hint("Minus vor einer Klammer: Beim Weglassen der Klammer drehen sich alle Vorzeichen darin um."),
        false => step,
    });
    if result.is_empty() {
        Ok(Term::zero())
    } else {
        Term::expanded(result, Part::one())
    }
}

fn common_denominator_step(a: &Part, b: &Part, common: &Part, verb: &str) -> Step {
    let mut lines = vec![
        format!(
            "Die Nenner {} und {} sind verschieden. Brüche kann man nur {verb}, wenn sie den gleichen Nenner haben.",
            factor_text(a),
            factor_text(b)
        ),
        "Der Hauptnenner enthält jeden Faktor, der in einem der Nenner vorkommt – jeden mit seiner höchsten Hochzahl:".to_string(),
    ];
    if a.number > 1 || b.number > 1 {
        lines.push(format!(
            "Zahlen: kgV({}, {}) = {}",
            a.number, b.number, common.number
        ));
    }
    let power = |atom: &Atom, e: u32| {
        if e == 0 {
            "–".to_string()
        } else {
            atom.power_text(e)
        }
    };
    for (v, e) in common.vars.iter() {
        let atom = Atom::Var(v, 1);
        lines.push(format!(
            "{v}: im ersten Nenner {}, im zweiten {} → {}",
            power(&atom, a.vars.exponent(v)),
            power(&atom, b.vars.exponent(v)),
            atom.power_text(e)
        ));
    }
    let exponent = |part: &Part, p: &Poly| {
        part.polys
            .iter()
            .find(|(q, _)| q.same(p))
            .map_or(0, |(_, e)| *e)
    };
    for (p, e) in &common.polys {
        let atom = Atom::Poly(p.clone(), 1);
        lines.push(format!(
            "{}: im ersten Nenner {}, im zweiten {} → {}",
            atom.power_text(1),
            power(&atom, exponent(a, p)),
            power(&atom, exponent(b, p)),
            atom.power_text(*e)
        ));
    }
    lines.push(format!("Hauptnenner: {}", common.text()));
    let mut line = text_line("Hauptnenner = ");
    line.extend(common.sum().line());
    Step::new("Hauptnenner bestimmen", lines.join("\n"), line)
}

/// Factors the new numerator and cancels it against the common denominator, if possible.
fn result_cancel_step(
    result: Vec<Mono>,
    den: Part,
    steps: &mut Vec<Step>,
) -> Result<Term, AlgebraError> {
    let factored = Factored::from_terms(&result)?;
    let num = factored.part;
    let common = num.abs().common(&den);
    if common.is_one() {
        return Term::expanded(result, den);
    }
    let cancelled = Term::new(num.without(&common), den.without(&common), Shown::Factored);
    let mark = |part: &Part| -> Vec<Item> {
        let mut items = items(part);
        for item in &mut items {
            let shared = match &item.atom {
                Atom::Number(_) => common.number > 1,
                Atom::Var(v, _) => common.vars.exponent(*v) > 0,
                Atom::Poly(p, _) => common.polys.iter().any(|(q, _)| q.same(p)),
                Atom::Mono(_) => false,
            };
            if shared {
                strike(item, 0);
            }
        }
        items
    };
    let mut lines = Vec::new();
    if !factored.notes.is_empty() {
        lines.push(format!(
            "Zähler als Produkt schreiben: {}",
            factored.notes.join(", ")
        ));
    }
    lines.push(format!(
        "Zähler und Nenner haben den gemeinsamen Faktor {} ({}) – oben und unten streichen.",
        factor_text(&common),
        color_name(0)
    ));
    lines.push("Warum? Ein Ergebnis gibt man immer vollständig gekürzt an.".to_string());
    let before = bar_line(num.negative, &mark(&num.abs()), &mark(&den));
    steps.push(Step::new(
        "Ergebnis kürzen",
        lines.join("\n"),
        equals(before, cancelled.line(None)),
    ));
    Ok(cancelled)
}

#[cfg(test)]
mod tests {
    use super::super::steps;
    use super::*;

    fn result(input: &str) -> String {
        line_text(&steps(input).unwrap().last().unwrap().line)
    }

    fn titles(input: &str) -> Vec<String> {
        steps(input).unwrap().into_iter().map(|s| s.title).collect()
    }

    #[test]
    fn multiplies_and_divides_with_variables() {
        assert_eq!(result("2a/3b · 9b/4a"), "2a/3b · 9b/4a = 3/2");
        assert_eq!(result("x/y · y²/x³"), "x/y · y²/x³ = y/x²");
        assert_eq!(
            result("(x² - 9)/(x + 3) · 2/(x - 3)"),
            "(x² − 9)/(x + 3) · 2/(x − 3) = 2"
        );
        assert_eq!(
            result("4x/(x + 1) : 2x/(x + 1)²"),
            "4x/(x + 1) : 2x/((x + 1)²) = 2(x + 1)"
        );
        assert_eq!(result("-3a/5 · 10/a²"), "−3a/5 · 10/a² = −6/a");
        assert_eq!(result("x · 1/x²"), "x · 1/x² = 1/x");
        assert_eq!(
            titles("2a/3b · 9b/4a"),
            [
                "Auf einen Bruchstrich schreiben",
                "Vor dem Ausmultiplizieren kürzen",
                "Ergebnis"
            ]
        );
        assert_eq!(titles("(x² - 9)/(x + 3) · 2/(x - 3)")[0], "Faktorisieren");
        assert_eq!(titles("a/b : c/d")[0], "Mit dem Kehrwert multiplizieren");
    }

    #[test]
    fn adds_and_subtracts_with_variables() {
        assert_eq!(result("x/2 + x/3"), "x/2 + x/3 = 5x/6");
        assert_eq!(result("1/x + 1/y"), "1/x + 1/y = (x + y)/xy");
        assert_eq!(
            result("(x + 1)/2 - (x - 1)/3"),
            "(x + 1)/2 − (x − 1)/3 = (x + 5)/6"
        );
        assert_eq!(
            result("1/(x - 1) - 1/(x + 1)"),
            "1/(x − 1) − 1/(x + 1) = 2/((x − 1)(x + 1))"
        );
        assert_eq!(result("3/(2x) + 1/(4x²)"), "3/2x + 1/4x² = (6x + 1)/4x²");
        assert_eq!(result("x/(x + 1) + 1/(x + 1)"), "x/(x + 1) + 1/(x + 1) = 1");
        assert_eq!(result("a/b - a/b"), "a/b − a/b = 0");
        assert_eq!(result("x + 1/x"), "x + 1/x = (x² + 1)/x");
        assert_eq!(result("2x - (x - 3)"), "2x − (x − 3) = x + 3");
        assert_eq!(
            titles("1/x + 1/y"),
            [
                "Hauptnenner bestimmen",
                "Auf den Hauptnenner erweitern",
                "Zähler addieren",
                "Ergebnis"
            ]
        );
        let steps = steps("1/(x - 1) - 1/(x + 1)").unwrap();
        let common = steps
            .iter()
            .find(|s| s.title == "Hauptnenner bestimmen")
            .unwrap();
        assert_eq!(line_text(&common.line), "Hauptnenner = (x − 1)(x + 1)");
        assert!(steps
            .iter()
            .any(|s| s.hint.contains("Minus vor einer Klammer")));
    }

    #[test]
    fn factors_denominators_before_the_common_denominator() {
        let steps = steps("2/(x² - 1) + 1/(x + 1)").unwrap();
        assert_eq!(steps[0].title, "Faktorisieren");
        assert_eq!(
            line_text(&steps.last().unwrap().line),
            "2/(x² − 1) + 1/(x + 1) = 1/(x − 1)"
        );
        assert!(steps.iter().any(|s| s.title == "Ergebnis kürzen"));
    }

    #[test]
    fn calculates_longer_terms_in_order() {
        assert_eq!(result("x/2 + x/3 · 3/x"), "x/2 + x/3 · 3/x = (x + 2)/2");
        assert_eq!(result("(1/x + 1/y) · xy"), "(1/x + 1/y) · xy = x + y");
        assert_eq!(
            result("(a/b - b/a) : (a + b)/(ab)"),
            "(a/b − b/a) : (a + b)/ab = a − b"
        );
        let steps = steps("x/2 + x/3 · 3/x").unwrap();
        assert_eq!(steps[1].title, "Als Nächstes: x/3 · 3/x");
    }

    #[test]
    fn reports_input_errors() {
        assert_eq!(parse("x/2").unwrap_err(), AlgebraError::MissingOperator);
        assert_eq!(parse("x/0 + 1").unwrap_err(), AlgebraError::DivisionByZero);
        assert_eq!(
            parse("x/2/3 + 1").unwrap_err(),
            AlgebraError::SlashBetweenFractions
        );
        assert_eq!(parse("(x + 1/2").unwrap_err(), AlgebraError::MissingClose);
        assert_eq!(parse("x + 1)").unwrap_err(), AlgebraError::UnexpectedClose);
        assert_eq!(parse("x + ").unwrap_err(), AlgebraError::UnexpectedEnd);
        assert_eq!(parse("1,5 + x").unwrap_err(), AlgebraError::NotInteger);
        assert_eq!(
            parse("ä + 1").unwrap_err(),
            AlgebraError::UnexpectedChar('ä')
        );
        assert_eq!(
            steps("x : (1/x - 1/x)").unwrap_err(),
            AlgebraError::DivisionByZero
        );
    }
}
