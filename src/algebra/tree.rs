//! The working form of a term — a sum of products whose factors are monomials or
//! bracketed sums — and its rendering as coloured tokens.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use super::parse::{Expr, Op};
use super::rational::Rational;
use super::{AlgebraError, Glyph, Line, Token};

/// How one node is highlighted in the current step.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Mark {
    /// Marker colour index, drawn as an underline.
    pub color: Option<usize>,
    /// Emphasise the sign, because the step changed or decided it.
    pub sign: bool,
}

impl Mark {
    pub fn color(index: usize) -> Self {
        Self { color: Some(index), sign: false }
    }
}

/// The variable part of a monomial: variable → exponent (never 0), alphabetical.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Vars(BTreeMap<char, u32>);

impl Vars {
    pub fn single(var: char, exponent: u32) -> Self {
        Self(if exponent == 0 { BTreeMap::new() } else { BTreeMap::from([(var, exponent)]) })
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn degree(&self) -> u32 {
        self.0.values().sum()
    }

    pub fn exponent(&self, var: char) -> u32 {
        self.0.get(&var).copied().unwrap_or(0)
    }

    pub fn iter(&self) -> impl Iterator<Item = (char, u32)> + '_ {
        self.0.iter().map(|(&v, &e)| (v, e))
    }

    pub fn mul(&self, other: &Self) -> Self {
        let mut vars = self.0.clone();
        for (v, e) in other.iter() {
            *vars.entry(v).or_insert(0) += e;
        }
        Self(vars)
    }

    /// Variables present in both, each with the smaller exponent.
    pub fn common(&self, other: &Self) -> Self {
        Self(self.iter().filter_map(|(v, e)| other.0.get(&v).map(|&f| (v, e.min(f)))).collect())
    }

    /// `self / other`, where `other` divides `self`.
    pub fn div(&self, other: &Self) -> Self {
        Self(self.iter().filter_map(|(v, e)| e.checked_sub(other.exponent(v)).filter(|&r| r > 0).map(|r| (v, r))).collect())
    }

    pub fn text(&self) -> String {
        self.iter().map(|(v, e)| if e == 1 { v.to_string() } else { format!("{v}{}", superscript(e)) }).collect()
    }

    /// Standard order: higher degree first, then the larger exponent of the
    /// alphabetically earlier variable first (`x² , xy, y², x, y, 1`).
    pub fn standard_cmp(&self, other: &Self) -> Ordering {
        other.degree().cmp(&self.degree()).then_with(|| {
            let vars: BTreeSet<char> = self.0.keys().chain(other.0.keys()).copied().collect();
            vars.into_iter()
                .map(|v| other.exponent(v).cmp(&self.exponent(v)))
                .find(|o| o.is_ne())
                .unwrap_or(Ordering::Equal)
        })
    }
}

pub fn superscript(n: u32) -> String {
    const DIGITS: [char; 10] = ['⁰', '¹', '²', '³', '⁴', '⁵', '⁶', '⁷', '⁸', '⁹'];
    n.to_string().bytes().map(|b| DIGITS[usize::from(b - b'0')]).collect()
}

/// A coefficient times a product of variable powers, e.g. `−3x²y`.
#[derive(Debug, Clone)]
pub struct Mono {
    pub coeff: Rational,
    pub vars: Vars,
    pub mark: Mark,
}

impl Mono {
    pub fn new(coeff: Rational, vars: Vars) -> Self {
        Self { coeff, vars, mark: Mark::default() }
    }

    pub fn constant(coeff: Rational) -> Self {
        Self::new(coeff, Vars::default())
    }

    pub fn power(var: char, exponent: u32) -> Self {
        Self::new(Rational::ONE, Vars::single(var, exponent))
    }

    pub fn is_constant(&self) -> bool {
        self.vars.is_empty()
    }

    pub fn mul(&self, other: &Self) -> Self {
        Self::new(self.coeff * other.coeff, self.vars.mul(&other.vars))
    }

    /// `self / other`, where the variables of `other` divide those of `self`.
    pub fn div(&self, other: &Self) -> Self {
        Self::new(self.coeff / other.coeff, self.vars.div(&other.vars))
    }

    pub fn neg(&self) -> Self {
        Self::new(-self.coeff, self.vars.clone())
    }

    pub fn marked(mut self, mark: Mark) -> Self {
        self.mark = mark;
        self
    }

    /// The monomial as written at the start of a term, e.g. `−5x`.
    pub fn text(&self) -> String {
        Sum::from_terms(vec![self.clone()]).text()
    }

    /// The monomial with an explicit sign, e.g. `+5x` or `−5x`.
    pub fn signed_text(&self) -> String {
        if self.coeff.is_negative() { self.text() } else { format!("+{}", self.text()) }
    }
}

#[derive(Debug, Clone)]
pub enum Factor {
    Mono(Mono),
    Group(Group),
}

/// A bracketed sum, optionally raised to a power.
#[derive(Debug, Clone)]
pub struct Group {
    pub sum: Sum,
    pub power: u32,
}

impl Group {
    /// The bracket as factors: a bracket around a single term is just that term.
    pub fn into_factors(self) -> Vec<Factor> {
        match self.sum.0.len() {
            0 => vec![Factor::Mono(Mono::constant(Rational::ZERO))],
            1 if self.power == 1 => self.sum.0.into_iter().flat_map(|s| s.factors).collect(),
            _ => vec![Factor::Group(self)],
        }
    }
}

/// A product of factors; the sign is the sign of the leading monomial.
#[derive(Debug, Clone, Default)]
pub struct Summand {
    pub factors: Vec<Factor>,
    pub mark: Mark,
}

impl Summand {
    pub fn new(factors: Vec<Factor>) -> Self {
        Self { factors, mark: Mark::default() }
    }

    pub fn mono(mono: Mono) -> Self {
        Self::new(vec![Factor::Mono(mono)])
    }

    pub fn marked(mut self, mark: Mark) -> Self {
        self.mark = mark;
        self
    }

    /// Negates the term as it is written: `3x` becomes `−3x`, but `−3` becomes `−(−3)`.
    pub fn negate(&mut self) {
        match self.factors.first_mut() {
            Some(Factor::Mono(m)) if !m.coeff.is_negative() => m.coeff = -m.coeff,
            _ => self.factors.insert(0, Factor::Mono(Mono::constant(-Rational::ONE))),
        }
    }

    /// Flips the sign of the leading monomial: `3x` ↔ `−3x`.
    pub fn flip(&mut self) {
        match self.factors.first_mut() {
            Some(Factor::Mono(m)) => m.coeff = -m.coeff,
            _ => self.factors.insert(0, Factor::Mono(Mono::constant(-Rational::ONE))),
        }
    }

    pub fn groups(&self) -> impl Iterator<Item = &Group> {
        self.factors.iter().filter_map(|f| match f {
            Factor::Group(g) => Some(g),
            Factor::Mono(_) => None,
        })
    }

    pub fn groups_mut(&mut self) -> impl Iterator<Item = &mut Group> {
        self.factors.iter_mut().filter_map(|f| match f {
            Factor::Group(g) => Some(g),
            Factor::Mono(_) => None,
        })
    }

    pub fn as_mono(&self) -> Option<&Mono> {
        match self.factors.as_slice() {
            [Factor::Mono(m)] => Some(m),
            _ => None,
        }
    }

    /// The term as written at the start of a sum.
    pub fn text(&self) -> String {
        Sum(vec![self.clone()]).text()
    }
}

#[derive(Debug, Clone, Default)]
pub struct Sum(pub Vec<Summand>);

impl Sum {
    pub fn from_terms(terms: Vec<Mono>) -> Self {
        Sum(terms.into_iter().map(Summand::mono).collect())
    }

    /// Converts a parsed term. Brackets and explicit products are kept so that they can
    /// be resolved step by step; only juxtaposed numbers and variables (`3x²`) merge.
    pub fn from_expr(expr: &Expr) -> Result<Self, AlgebraError> {
        Ok(match expr {
            Expr::Sum(terms) => {
                let mut summands = Vec::new();
                for term in terms {
                    summands.extend(Self::from_expr(term)?.0);
                }
                Sum(summands)
            }
            Expr::Neg(inner) => {
                let mut sum = Self::from_expr(inner)?;
                sum.0.iter_mut().for_each(Summand::negate);
                sum
            }
            _ => Sum(vec![Summand::new(product_factors(expr)?)]),
        })
    }

    /// The monomials, if every summand is a single monomial.
    pub fn terms(&self) -> Option<Vec<Mono>> {
        self.0.iter().map(|s| s.as_mono().cloned()).collect()
    }

    /// No summand contains a bracket.
    pub fn is_flat(&self) -> bool {
        self.0.iter().all(|s| s.groups().next().is_none())
    }

    pub fn clear_marks(&mut self) {
        for summand in &mut self.0 {
            summand.mark = Mark::default();
            for factor in &mut summand.factors {
                match factor {
                    Factor::Mono(m) => m.mark = Mark::default(),
                    Factor::Group(g) => g.sum.clear_marks(),
                }
            }
        }
    }

    pub fn mono_count(&self) -> usize {
        self.0
            .iter()
            .flat_map(|s| &s.factors)
            .map(|f| match f {
                Factor::Mono(_) => 1,
                Factor::Group(g) => g.sum.mono_count(),
            })
            .sum()
    }

    /// Calls `f` for every summand, outer ones before the ones inside their brackets.
    pub fn visit_mut<F: FnMut(&mut Summand)>(&mut self, f: &mut F) {
        for summand in &mut self.0 {
            f(summand);
            for group in summand.groups_mut() {
                group.sum.visit_mut(f);
            }
        }
    }

    /// The sum nested `path` brackets deep; each step is (summand index, factor index).
    pub fn at_path_mut(&mut self, path: &[(usize, usize)]) -> &mut Sum {
        path.iter().fold(self, |sum, &(i, j)| match &mut sum.0[i].factors[j] {
            Factor::Group(group) => &mut group.sum,
            Factor::Mono(_) => unreachable!("paths only lead through brackets"),
        })
    }

    pub fn line(&self) -> Line {
        let mut line = Line::new();
        render_sum(self, &mut line, None);
        line
    }

    pub fn text(&self) -> String {
        line_text(&self.line())
    }
}

fn product_factors(expr: &Expr) -> Result<Vec<Factor>, AlgebraError> {
    let mut factors = Vec::new();
    match expr {
        Expr::Product(items) => {
            for (op, item) in items {
                push_factor(&mut factors, *op, item)?;
            }
        }
        other => push_factor(&mut factors, Op::Implicit, other)?,
    }
    Ok(factors)
}

fn push_factor(factors: &mut Vec<Factor>, op: Op, expr: &Expr) -> Result<(), AlgebraError> {
    if op == Op::Divide {
        let divisor = constant_value(expr)?;
        if divisor.is_zero() {
            return Err(AlgebraError::DivisionByZero);
        }
        // `x/2` is read as the monomial ½x.
        match factors.last_mut() {
            Some(Factor::Mono(m)) => m.coeff = m.coeff / divisor,
            _ => factors.push(Factor::Mono(Mono::constant(divisor.recip()))),
        }
        return Ok(());
    }
    let merge = op == Op::Implicit && !matches!(expr, Expr::Paren(_));
    for factor in item_factors(expr)? {
        if merge {
            if let (Some(Factor::Mono(last)), Factor::Mono(m)) = (factors.last_mut(), &factor) {
                *last = last.mul(m);
                continue;
            }
        }
        factors.push(factor);
    }
    Ok(())
}

fn item_factors(expr: &Expr) -> Result<Vec<Factor>, AlgebraError> {
    Ok(match expr {
        Expr::Num(n) => vec![Factor::Mono(Mono::constant(*n))],
        Expr::Var(v) => vec![Factor::Mono(Mono::power(*v, 1))],
        Expr::Power(base, exponent) => match base.as_ref() {
            Expr::Var(v) => vec![Factor::Mono(Mono::power(*v, *exponent))],
            Expr::Num(n) => vec![Factor::Mono(Mono::constant(n.pow(*exponent)))],
            Expr::Paren(inner) => vec![Factor::Group(Group { sum: Sum::from_expr(inner)?, power: *exponent })],
            other => vec![Factor::Group(Group { sum: Sum::from_expr(other)?, power: *exponent })],
        },
        Expr::Paren(inner) => Group { sum: Sum::from_expr(inner)?, power: 1 }.into_factors(),
        Expr::Neg(inner) => {
            let mut summand = Summand::new(item_factors(inner)?);
            summand.negate();
            summand.factors
        }
        Expr::Product(_) => product_factors(expr)?,
        Expr::Sum(_) => vec![Factor::Group(Group { sum: Sum::from_expr(expr)?, power: 1 })],
    })
}

fn constant_value(expr: &Expr) -> Result<Rational, AlgebraError> {
    Ok(match expr {
        Expr::Num(n) => *n,
        Expr::Var(_) => return Err(AlgebraError::DivisionByVariable),
        Expr::Neg(inner) => -constant_value(inner)?,
        Expr::Paren(inner) => constant_value(inner)?,
        Expr::Power(base, exponent) => constant_value(base)?.pow(*exponent),
        Expr::Sum(terms) => terms.iter().try_fold(Rational::ZERO, |acc, t| Ok::<_, AlgebraError>(acc + constant_value(t)?))?,
        Expr::Product(items) => items.iter().try_fold(Rational::ONE, |acc, (op, item)| {
            let value = constant_value(item)?;
            match op {
                Op::Divide if value.is_zero() => Err(AlgebraError::DivisionByZero),
                Op::Divide => Ok(acc / value),
                _ => Ok(acc * value),
            }
        })?,
    })
}

pub fn push_text(line: &mut Line, text: impl Into<String>, color: Option<usize>, sign: bool) {
    line.push(Token { glyph: Glyph::Text(text.into()), color, sign });
}

/// A number with its sign; non-integers are drawn as stacked fractions.
pub fn push_number(line: &mut Line, value: Rational, color: Option<usize>, sign: bool) {
    let negative = value.is_negative();
    if negative {
        push_text(line, "−", color, sign);
    }
    let value = value.abs();
    let glyph = if value.is_integer() || value.is_overflow() {
        Glyph::Text(value.to_string())
    } else {
        Glyph::Fraction(value.numer().to_string(), value.denom().to_string())
    };
    line.push(Token { glyph, color, sign: sign && !negative });
}

pub fn text_line(text: impl Into<String>) -> Line {
    let mut line = Line::new();
    push_text(&mut line, text, None, false);
    line
}

pub fn line_text(line: &Line) -> String {
    line.iter()
        .map(|token| match &token.glyph {
            Glyph::Text(text) => text.clone(),
            Glyph::Fraction(num, den) => format!("{num}/{den}"),
        })
        .collect()
}

fn render_sum(sum: &Sum, line: &mut Line, inherited: Option<usize>) {
    if sum.0.is_empty() {
        push_text(line, "0", inherited, false);
    }
    for (i, summand) in sum.0.iter().enumerate() {
        render_summand(summand, i == 0, line, inherited);
    }
}

fn render_summand(summand: &Summand, first: bool, line: &mut Line, inherited: Option<usize>) {
    let color = summand.mark.color.or(inherited);
    let lead = match summand.factors.first() {
        Some(Factor::Mono(m)) => Some(m),
        _ => None,
    };
    let negative = lead.is_some_and(|m| m.coeff.is_negative());
    let sign = summand.mark.sign || lead.is_some_and(|m| m.mark.sign);
    let sign_color = lead.and_then(|m| m.mark.color).or(color);

    if !first {
        push_text(line, " ", None, false);
    }
    let body = line.len();
    match (first, negative) {
        (true, false) => {}
        (true, true) => push_text(line, "−", sign_color, sign),
        (false, true) => push_text(line, "− ", sign_color, sign),
        (false, false) => push_text(line, "+ ", sign_color, sign),
    }

    // A leading ±1 before a bracket or a negative number is shown as its sign only:
    // `−(a − b)`, `−(−3)`. Before anything else it stays visible: `−1·x`.
    let unit = lead.is_some_and(|m| m.is_constant() && m.coeff.abs().is_one());
    let hide_unit = unit
        && match summand.factors.get(1) {
            Some(Factor::Group(_)) => true,
            Some(Factor::Mono(next)) => next.coeff.is_negative(),
            None => false,
        };
    for (k, factor) in summand.factors.iter().enumerate() {
        match factor {
            Factor::Mono(m) if k == 0 => {
                if !hide_unit {
                    render_mono_abs(m, line, color);
                }
            }
            Factor::Mono(m) => {
                if !(k == 1 && hide_unit) {
                    push_text(line, "·", color, false);
                }
                if m.coeff.is_negative() {
                    let c = m.mark.color.or(color);
                    push_text(line, "(", c, false);
                    push_text(line, "−", c, m.mark.sign);
                    render_mono_abs(m, line, color);
                    push_text(line, ")", c, false);
                } else {
                    render_mono_abs(m, line, color);
                }
            }
            Factor::Group(group) => {
                push_text(line, "(", color, false);
                render_sum(&group.sum, line, color);
                let close = if group.power == 1 { ")".to_string() } else { format!("){}", superscript(group.power)) };
                push_text(line, close, color, false);
            }
        }
    }

    // Without a visible sign, emphasise the start of the term instead.
    if sign && first && !negative {
        if let Some(token) = line.get_mut(body) {
            token.sign = true;
        }
    }
}

fn render_mono_abs(mono: &Mono, line: &mut Line, inherited: Option<usize>) {
    let color = mono.mark.color.or(inherited);
    let coeff = mono.coeff.abs();
    if mono.vars.is_empty() || !coeff.is_one() {
        push_number(line, coeff, color, false);
    }
    if !mono.vars.is_empty() {
        push_text(line, mono.vars.text(), color, false);
    }
}

#[cfg(test)]
mod tests {
    use super::super::parse;
    use super::*;

    fn shown(input: &str) -> String {
        Sum::from_expr(&parse::expression(input).unwrap()).unwrap().text()
    }

    #[test]
    fn keeps_the_written_structure() {
        assert_eq!(shown("3x + 2y - 5x"), "3x + 2y − 5x");
        assert_eq!(shown("-(a - 2b)"), "−(a − 2b)");
        assert_eq!(shown("4 - (-3)"), "4 − (−3)");
        assert_eq!(shown("2*(-3)"), "2·(−3)");
        assert_eq!(shown("3a·4b"), "3a·4b");
        assert_eq!(shown("-2(a - 4b)"), "−2(a − 4b)");
        assert_eq!(shown("(x+3)(x-2)"), "(x + 3)(x − 2)");
        assert_eq!(shown("(a + b)^2"), "(a + b)²");
        assert_eq!(shown("x/2 + 2x²y"), "1/2x + 2x²y");
        assert_eq!(shown("((x))"), "x");
    }

    #[test]
    fn orders_monomials_by_degree_then_alphabet() {
        let vars = |input: &str| Sum::from_expr(&parse::expression(input).unwrap()).unwrap().terms().unwrap()[0].vars.clone();
        let mut list = ["y", "1", "xy", "x", "x²", "y²"].map(vars);
        list.sort_by(Vars::standard_cmp);
        let texts: Vec<String> = list.iter().map(Vars::text).collect();
        assert_eq!(texts, ["x²", "xy", "y²", "x", "y", ""]);
    }
}
