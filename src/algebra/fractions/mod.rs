//! Cancelling ("kürzen") and expanding ("erweitern") fractions step by step.
//!
//! Numerator and denominator are brought into a factored form ([`Part`]): a sign, a
//! number, variable powers and brackets that cannot be split further. Cancelling divides
//! both by their common part; expanding multiplies both by the same factor.

mod extend;
mod reduce;

use std::collections::BTreeSet;

pub use extend::steps as extend_steps;
pub use reduce::{steps as reduce_steps, Style};

use super::factor::common_factor;
use super::parse::{self, Expr};
use super::rational::Rational;
use super::simplify;
use super::tree::{push_text, superscript, Factor, Group, Mono, Sum, Summand};
use super::{AlgebraError, Glyph, Line, Token};

/// Largest number allowed in a factored numerator or denominator.
const LIMIT: i128 = 1_000_000_000_000;

fn unparen(expr: &Expr) -> &Expr {
    match expr {
        Expr::Paren(inner) => unparen(inner),
        other => other,
    }
}

/// Splits `numerator / denominator` at the one fraction bar outside brackets.
fn parse_fraction(text: &str) -> Result<(Expr, Expr), AlgebraError> {
    if text.trim().is_empty() {
        return Err(AlgebraError::Empty);
    }
    let mut depth = 0_i32;
    let mut bars = Vec::new();
    for (i, c) in text.char_indices() {
        match c {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            '/' | ':' | '÷' | '⁄' if depth == 0 => bars.push((i, c.len_utf8())),
            _ => {}
        }
    }
    let &[(at, len)] = bars.as_slice() else {
        return Err(if bars.is_empty() { AlgebraError::MissingFractionBar } else { AlgebraError::TooManyFractionBars });
    };
    let (top, bottom) = (text[..at].trim(), text[at + len..].trim());
    if top.is_empty() || bottom.is_empty() {
        return Err(AlgebraError::EmptyFractionPart);
    }
    Ok((parse::expression(top)?, parse::expression(bottom)?))
}

fn numbers_only(exprs: &[&Expr]) -> Result<(), AlgebraError> {
    let mut vars = BTreeSet::new();
    exprs.iter().for_each(|e| e.collect_variables(&mut vars));
    if vars.is_empty() { Ok(()) } else { Err(AlgebraError::VariablesNotAllowed) }
}

/// The sum multiplied out and combined, in standard order (`x² − 6x + 9`).
fn normalized(sum: Sum) -> Result<Vec<Mono>, AlgebraError> {
    let (_, mut terms) = simplify::normalize(sum)?;
    terms.sort_by(|a, b| a.vars.standard_cmp(&b.vars));
    Ok(terms)
}

/// The term multiplied out and combined, with integer coefficients only.
fn terms(expr: &Expr) -> Result<Vec<Mono>, AlgebraError> {
    let terms = normalized(Sum::from_expr(unparen(expr))?)?;
    if terms.iter().any(|t| !t.coeff.is_integer()) {
        return Err(AlgebraError::NotInteger);
    }
    Ok(terms)
}

/// The term as it was written, without outer brackets.
fn written(expr: &Expr) -> Result<Line, AlgebraError> {
    Ok(Sum::from_expr(unparen(expr))?.line())
}

fn terms_text(terms: &[Mono]) -> String {
    Sum::from_terms(terms.to_vec()).text()
}

/// A sum of at least two terms in standard order, with a positive first term and no
/// common factor: a bracket that can only be cancelled as a whole.
#[derive(Debug, Clone)]
struct Poly(Vec<Mono>);

impl Poly {
    fn same(&self, other: &Self) -> bool {
        self.0.len() == other.0.len() && self.0.iter().zip(&other.0).all(|(a, b)| a.coeff == b.coeff && a.vars == b.vars)
    }

    fn sum(&self) -> Sum {
        Sum::from_terms(self.0.clone())
    }

    fn group(&self, power: u32) -> Factor {
        Factor::Group(Group { sum: self.sum(), power })
    }
}

/// A numerator or denominator as a product: sign · number · variables · brackets.
#[derive(Debug, Clone)]
struct Part {
    negative: bool,
    /// 0 if the whole part is 0.
    number: i128,
    vars: super::tree::Vars,
    polys: Vec<(Poly, u32)>,
}

impl Part {
    fn one() -> Self {
        Self { negative: false, number: 1, vars: Default::default(), polys: Vec::new() }
    }

    fn is_zero(&self) -> bool {
        self.number == 0
    }

    fn is_one(&self) -> bool {
        self.number == 1 && self.vars.is_empty() && self.polys.is_empty()
    }

    fn abs(&self) -> Self {
        Self { negative: false, ..self.clone() }
    }

    fn mul_mono(&mut self, mono: &Mono) -> Result<(), AlgebraError> {
        if mono.coeff.is_overflow() {
            return Err(AlgebraError::NumbersTooLarge);
        }
        if !mono.coeff.is_integer() {
            return Err(AlgebraError::NotInteger);
        }
        self.number = self
            .number
            .checked_mul(mono.coeff.numer().abs())
            .filter(|&n| n <= LIMIT)
            .ok_or(AlgebraError::NumbersTooLarge)?;
        self.negative ^= mono.coeff.is_negative();
        self.vars = self.vars.mul(&mono.vars);
        Ok(())
    }

    fn mul_poly(&mut self, poly: Poly, power: u32) {
        match self.polys.iter_mut().find(|(p, _)| p.same(&poly)) {
            Some((_, e)) => *e += power,
            None => self.polys.push((poly, power)),
        }
    }

    /// The largest factor of both parts (without sign).
    fn common(&self, other: &Self) -> Self {
        let polys = self
            .polys
            .iter()
            .filter_map(|(p, e)| other.polys.iter().find(|(q, _)| q.same(p)).map(|(_, f)| (p.clone(), *e.min(f))))
            .collect();
        Self {
            negative: false,
            number: super::rational::gcd(self.number, other.number),
            vars: self.vars.common(&other.vars),
            polys,
        }
    }

    /// `self / divisor`, where every factor of `divisor` occurs in `self`.
    fn without(&self, divisor: &Self) -> Self {
        let mut polys = self.polys.clone();
        for (p, e) in &divisor.polys {
            if let Some((_, f)) = polys.iter_mut().find(|(q, _)| q.same(p)) {
                *f = f.saturating_sub(*e);
            }
        }
        polys.retain(|(_, e)| *e > 0);
        Self {
            negative: self.negative != divisor.negative,
            number: self.number / divisor.number.max(1),
            vars: self.vars.div(&divisor.vars),
            polys,
        }
    }

    /// `self / divisor`, if that is again a product without fraction.
    fn divide(&self, divisor: &Self) -> Option<Self> {
        let divides = divisor.number != 0
            && self.number % divisor.number == 0
            && divisor.vars.common(&self.vars) == divisor.vars
            && divisor.polys.iter().all(|(p, e)| self.polys.iter().any(|(q, f)| q.same(p) && f >= e));
        divides.then(|| self.without(divisor))
    }

    /// The part without its sign in the usual notation, e.g. `4x(x + 2)²`.
    fn sum(&self) -> Sum {
        if let [(poly, 1)] = self.polys.as_slice() {
            if self.number == 1 && self.vars.is_empty() {
                return poly.sum();
            }
        }
        let mut factors = Vec::new();
        if self.number != 1 || !self.vars.is_empty() || self.polys.is_empty() {
            factors.push(Factor::Mono(Mono::new(Rational::int(self.number), self.vars.clone())));
        }
        factors.extend(self.polys.iter().map(|(p, e)| p.group(*e)));
        Sum(vec![Summand::new(factors)])
    }

    fn text(&self) -> String {
        self.sum().text()
    }

    /// The part with its sign; a negative sum is put in brackets: `−(x + 2)`.
    fn signed_line(&self) -> Line {
        let sum = self.sum();
        let mut line = Line::new();
        if !self.negative {
            return sum.line();
        }
        push_text(&mut line, "−", None, false);
        if sum.0.len() > 1 {
            push_text(&mut line, "(", None, false);
            line.extend(sum.line());
            push_text(&mut line, ")", None, false);
        } else {
            line.extend(sum.line());
        }
        line
    }

    /// The signed part multiplied out.
    fn expand(&self) -> Result<Vec<Mono>, AlgebraError> {
        let mut sum = self.sum();
        if self.negative {
            sum.0.iter_mut().for_each(Summand::negate);
        }
        normalized(sum)
    }
}

/// A numerator or denominator in factored form, with notes on what had to be factored.
struct Factored {
    part: Part,
    /// One line per bracket or sum that was rewritten, e.g. `4x + 8 = 4(x + 2)`.
    notes: Vec<String>,
    /// A negative number was factored out, which flips the signs in the bracket.
    negative_content: bool,
}

impl Factored {
    fn new(expr: &Expr) -> Result<Self, AlgebraError> {
        let sum = Sum::from_expr(unparen(expr))?;
        let mut out = Self { part: Part::one(), notes: Vec::new(), negative_content: false };
        match sum.0.as_slice() {
            [summand] => {
                for factor in &summand.factors {
                    match factor {
                        Factor::Mono(m) => out.part.mul_mono(m)?,
                        Factor::Group(g) => out.factor_sum(&g.sum, g.power)?,
                    }
                }
            }
            _ => out.factor_sum(&sum, 1)?,
        }
        Ok(out)
    }

    /// Multiplies `sum^power` into the part: common factor out, then binomial formulas.
    fn factor_sum(&mut self, sum: &Sum, power: u32) -> Result<(), AlgebraError> {
        let terms = normalized(sum.clone())?;
        let mut chain = vec![sum.text()];
        let simplified = terms_text(&terms);
        if simplified != chain[0] {
            chain.push(simplified);
        }
        match terms.as_slice() {
            [] => self.part.number = 0,
            [mono] => {
                for _ in 0..power {
                    self.part.mul_mono(mono)?;
                }
            }
            _ => {
                let content = common_factor(&terms);
                let primitive: Vec<Mono> = terms.iter().map(|t| t.div(&content)).collect();
                for _ in 0..power {
                    self.part.mul_mono(&content)?;
                }
                let lead = (!(content.is_constant() && content.coeff.is_one())).then(|| Factor::Mono(content.clone()));
                let factored = |factors: Vec<Factor>| Sum(vec![Summand::new(lead.iter().cloned().chain(factors).collect())]).text();
                if lead.is_some() {
                    chain.push(factored(vec![Poly(primitive.clone()).group(1)]));
                    self.negative_content |= content.coeff.is_negative();
                }
                match binomial(&primitive) {
                    Some((factors, name)) => {
                        chain.push(format!("{} ({name})", factored(factors.iter().map(|(p, e)| p.group(*e)).collect())));
                        for (poly, e) in factors {
                            self.part.mul_poly(poly, e * power);
                        }
                    }
                    None => self.part.mul_poly(Poly(primitive), power),
                }
            }
        }
        if chain.len() > 1 {
            self.notes.push(chain.join(" = "));
        }
        Ok(())
    }
}

/// The square root of a monomial with a positive square coefficient and even exponents.
fn mono_sqrt(mono: &Mono) -> Option<Mono> {
    let coeff = mono.coeff.sqrt().filter(|c| c.is_integer() && !c.is_zero())?;
    Some(Mono::new(coeff, mono.vars.halved()?))
}

/// Recognises the binomial formulas in a sum without common factor.
fn binomial(terms: &[Mono]) -> Option<(Vec<(Poly, u32)>, &'static str)> {
    match terms {
        [a, b] if b.coeff.is_negative() => {
            let (a, b) = (mono_sqrt(a)?, mono_sqrt(&b.neg())?);
            Some((vec![(Poly(vec![a.clone(), b.clone()]), 1), (Poly(vec![a, b.neg()]), 1)], "3. binomische Formel"))
        }
        [a, middle, c] => {
            let (a, c) = (mono_sqrt(a)?, mono_sqrt(c)?);
            let double = a.mul(&c).mul(&Mono::constant(Rational::int(2)));
            if middle.vars != double.vars {
                return None;
            }
            if middle.coeff == double.coeff {
                Some((vec![(Poly(vec![a, c]), 2)], "1. binomische Formel"))
            } else if middle.coeff == -double.coeff {
                Some((vec![(Poly(vec![a, c.neg()]), 2)], "2. binomische Formel"))
            } else {
                None
            }
        }
        _ => None,
    }
}

/// One factor in a product written with `·`.
#[derive(Debug, Clone)]
enum Atom {
    Number(i128),
    Var(char, u32),
    Mono(Mono),
    Poly(Poly, u32),
}

impl Atom {
    /// The same base: equal numbers, the same variable or the same bracket.
    fn same_base(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Number(a), Self::Number(b)) => a == b,
            (Self::Var(v, _), Self::Var(w, _)) => v == w,
            (Self::Poly(p, _), Self::Poly(q, _)) => p.same(q),
            _ => false,
        }
    }

    fn exponent(&self) -> u32 {
        match self {
            Self::Var(_, e) | Self::Poly(_, e) => *e,
            Self::Number(_) | Self::Mono(_) => 1,
        }
    }

    /// The factor raised to `exponent` (shown without exponent 1).
    fn power_text(&self, exponent: u32) -> String {
        let power = if exponent == 1 { String::new() } else { superscript(exponent) };
        match self {
            Self::Number(n) => format!("{n}{power}"),
            Self::Var(v, _) => format!("{v}{power}"),
            Self::Mono(m) => m.text(),
            Self::Poly(p, _) => format!("({}){power}", p.sum().text()),
        }
    }

    fn text(&self) -> String {
        self.power_text(self.exponent())
    }

    /// The term as one factor: a single monomial, or a bracket around a sum.
    fn of_terms(terms: &[Mono]) -> Self {
        match terms {
            [] => Self::Number(0),
            [mono] => Self::Mono(mono.clone()),
            _ => Self::Poly(Poly(terms.to_vec()), 1),
        }
    }
}

#[derive(Debug, Clone)]
struct Item {
    atom: Atom,
    color: Option<usize>,
    struck: bool,
    /// Highlight the sign of a negative factor.
    sign: bool,
    /// Shows the exponent as a subtraction, `x⁵⁻³`.
    minus: Option<u32>,
}

impl Item {
    fn new(atom: Atom) -> Self {
        Self { atom, color: None, struck: false, sign: false, minus: None }
    }

    fn colored(mut self, color: usize) -> Self {
        self.color = Some(color);
        self
    }
}

/// Factors joined by `·`, or `1` if there are none.
fn items_line(items: &[Item]) -> Line {
    let mut line = Line::new();
    if items.is_empty() {
        push_text(&mut line, "1", None, false);
    }
    for (k, item) in items.iter().enumerate() {
        if k > 0 {
            push_text(&mut line, "·", None, false);
        }
        let start = line.len();
        let color = item.color;
        let exponent = |e: u32| {
            let base = if e == 1 && item.minus.is_none() { String::new() } else { superscript(e) };
            item.minus.map_or(base.clone(), |m| format!("{base}⁻{}", superscript(m)))
        };
        match &item.atom {
            Atom::Number(n) => push_text(&mut line, n.to_string(), color, false),
            Atom::Var(v, e) => push_text(&mut line, format!("{v}{}", exponent(*e)), color, false),
            Atom::Mono(m) if m.coeff.is_negative() => {
                let bracket = k > 0;
                if bracket {
                    push_text(&mut line, "(", color, false);
                }
                push_text(&mut line, "−", color, item.sign);
                push_text(&mut line, m.neg().text(), color, false);
                if bracket {
                    push_text(&mut line, ")", color, false);
                }
            }
            Atom::Mono(m) => push_text(&mut line, m.text(), color, false),
            // One piece, so that cancelling strikes the whole bracket at once.
            Atom::Poly(p, e) => push_text(&mut line, format!("({}){}", p.sum().text(), exponent(*e)), color, false),
        }
        if item.struck {
            line[start..].iter_mut().for_each(|t| t.struck = true);
        }
    }
    line
}

fn stacked(top: Line, bottom: Line) -> Token {
    Token { glyph: Glyph::Stacked(top, bottom), color: None, sign: false, struck: false }
}

/// A fraction, with a minus sign in front if it is negative.
fn fraction_line(negative: bool, top: Line, bottom: Line) -> Line {
    let mut line = Line::new();
    if negative {
        push_text(&mut line, "−", None, false);
    }
    line.push(stacked(top, bottom));
    line
}

/// The fraction as it was entered.
fn written_fraction(top: &Expr, bottom: &Expr) -> Result<Line, AlgebraError> {
    Ok(fraction_line(false, written(top)?, written(bottom)?))
}

/// `left = right`.
fn equals(mut left: Line, right: Line) -> Line {
    push_text(&mut left, " = ", None, false);
    left.extend(right);
    left
}

#[cfg(test)]
mod tests {
    use super::super::tree::line_text;
    use super::*;

    fn factored(input: &str) -> String {
        let part = Factored::new(&parse::expression(input).unwrap()).unwrap().part;
        line_text(&part.signed_line())
    }

    #[test]
    fn factors_numerators_and_denominators() {
        assert_eq!(factored("12a²b"), "12a²b");
        assert_eq!(factored("4x + 8"), "4(x + 2)");
        assert_eq!(factored("x² - 9"), "(x + 3)(x − 3)");
        assert_eq!(factored("a² + 2ab + b²"), "(a + b)²");
        assert_eq!(factored("2x² - 12x + 18"), "2(x − 3)²");
        assert_eq!(factored("9 - 3x"), "−3(x − 3)");
        assert_eq!(factored("3(2x + 4)"), "6(x + 2)");
        assert_eq!(factored("(x + 1)(x + 1)"), "(x + 1)²");
        assert_eq!(factored("x² + 1"), "x² + 1");
    }

    #[test]
    fn splits_at_the_fraction_bar() {
        assert!(parse_fraction("12/18").is_ok());
        assert!(parse_fraction("(x/2)").is_err());
        assert_eq!(parse_fraction("12").unwrap_err(), AlgebraError::MissingFractionBar);
        assert_eq!(parse_fraction("1/2/3").unwrap_err(), AlgebraError::TooManyFractionBars);
        assert_eq!(parse_fraction("/3").unwrap_err(), AlgebraError::EmptyFractionPart);
    }
}
