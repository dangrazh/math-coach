//! Longer calculations with fractions: several operators and brackets.
//!
//! The calculation is done one operation at a time, in the order taught at school:
//! brackets first, then multiplication and division before addition and subtraction,
//! otherwise from left to right. Each operation is explained with the same steps as
//! a single calculation, and its result is then put back into the whole calculation.
//!
//! The order logic works for any [`Value`]: numbers here, fractions with variables in
//! [`symbolic`].

mod symbolic;

use std::cmp::Reverse;

use super::super::super::tree::{line_text, push_text};
use super::super::super::{color_name, AlgebraError, Glyph, Line, Step};
use super::{conversion_step, lex, product_steps, result_step, sum_steps, Frac, Operand, Operation, Parser, Tok, Written};

/// More operations would make the list of steps too long to follow.
const MAX_OPERATIONS: usize = 8;
const MAX_DEPTH: usize = 6;
/// Intermediate results stay below this, so that the next operation cannot overflow.
const MAX_VALUE: i128 = 1_000_000_000_000_000;

/// Something the calculation is done with: a number, or a fraction with variables.
trait Value: Clone + std::fmt::Debug {
    /// Drawn in the given marker colour.
    fn line(&self, color: Option<usize>) -> Line;

    fn text(&self) -> String {
        line_text(&self.line(None))
    }

    fn negated(&self) -> Self;

    /// A sum without fraction bar (`x + 1`), which needs brackets in a product.
    fn is_sum(&self) -> bool {
        false
    }

    /// The steps of `a op b`, appended to `steps`; returns the result.
    fn calculate(a: &Self, op: Operation, b: &Self, steps: &mut Vec<Step>) -> Result<Self, AlgebraError>;

    fn result_step(original: Line, value: &Self) -> Step;
}

impl Value for Operand {
    fn line(&self, color: Option<usize>) -> Line {
        self.colored(color)
    }

    fn text(&self) -> String {
        let number = match self.written {
            Written::Integer(n) => n.to_string(),
            Written::Fraction(n, d) => format!("{n}/{d}"),
            Written::Mixed(w, n, d) => format!("{w} {n}/{d}"),
        };
        if self.negative { format!("−{number}") } else { number }
    }

    fn negated(&self) -> Self {
        Operand { negative: !self.negative, ..*self }
    }

    fn calculate(a: &Self, op: Operation, b: &Self, steps: &mut Vec<Step>) -> Result<Self, AlgebraError> {
        steps.extend(conversion_step(*a, op, *b));
        let value = match op {
            Operation::Multiply | Operation::Divide => product_steps(a.value(), op, b.value(), steps)?,
            Operation::Add | Operation::Subtract => sum_steps(a.value(), op, b.value(), steps),
        };
        if value.num.abs() > MAX_VALUE || value.den > MAX_VALUE {
            return Err(AlgebraError::NumbersTooLarge);
        }
        Ok(operand(value))
    }

    fn result_step(original: Line, value: &Self) -> Step {
        result_step(original, value.value())
    }
}

/// An intermediate result, written without fraction bar if it is a whole number.
fn operand(value: Frac) -> Operand {
    let n = value.num.abs();
    let written = if value.den == 1 { Written::Integer(n) } else { Written::Fraction(n, value.den) };
    Operand { negative: value.negative(), written }
}

#[derive(Debug, Clone)]
enum Expr<V> {
    /// A value; `true` if it is marked in the current step.
    Num(V, bool),
    /// Brackets written by the user around a calculation.
    Group(Box<Expr<V>>),
    /// A minus sign in front of brackets.
    Neg(Box<Expr<V>>),
    Bin(Box<Expr<V>>, Operation, Box<Expr<V>>),
}

/// Brackets around a single value are dropped; they are drawn again where needed.
fn group<V>(inner: Expr<V>) -> Expr<V> {
    match inner {
        Expr::Num(..) => inner,
        _ => Expr::Group(Box::new(inner)),
    }
}

fn negate<V: Value>(expr: Expr<V>) -> Expr<V> {
    match expr {
        Expr::Num(value, marked) => Expr::Num(value.negated(), marked),
        Expr::Neg(inner) => *inner,
        _ => Expr::Neg(Box::new(expr)),
    }
}

impl Parser {
    /// Terms joined by `+` and `−`.
    fn sum(&mut self, depth: usize) -> Result<Expr<Operand>, AlgebraError> {
        let mut expr = self.product(depth)?;
        loop {
            let op = match self.peek() {
                Some(Tok::Plus) => Operation::Add,
                Some(Tok::Minus) => Operation::Subtract,
                _ => return Ok(expr),
            };
            self.pos += 1;
            expr = Expr::Bin(Box::new(expr), op, Box::new(self.product(depth)?));
        }
    }

    /// Factors joined by `·` and `:`.
    fn product(&mut self, depth: usize) -> Result<Expr<Operand>, AlgebraError> {
        let mut expr = self.factor(depth)?;
        loop {
            let op = match self.peek() {
                Some(Tok::Times) => Operation::Multiply,
                Some(Tok::Colon) => Operation::Divide,
                _ => return Ok(expr),
            };
            self.pos += 1;
            expr = Expr::Bin(Box::new(expr), op, Box::new(self.factor(depth)?));
        }
    }

    /// A number, or a calculation in brackets, with optional signs in front.
    fn factor(&mut self, depth: usize) -> Result<Expr<Operand>, AlgebraError> {
        match self.peek() {
            Some(Tok::Open) => {
                if depth >= MAX_DEPTH {
                    return Err(AlgebraError::TooDeep);
                }
                self.pos += 1;
                let inner = self.sum(depth + 1)?;
                match self.next() {
                    Some(Tok::Close) => Ok(group(inner)),
                    _ => Err(AlgebraError::MissingClose),
                }
            }
            Some(Tok::Minus) => {
                self.pos += 1;
                Ok(negate(self.factor(depth)?))
            }
            Some(Tok::Plus) => {
                self.pos += 1;
                self.factor(depth)
            }
            _ => Ok(Expr::Num(self.operand()?, false)),
        }
    }
}

fn parse(input: &str) -> Result<Expr<Operand>, AlgebraError> {
    let tokens = lex(input)?;
    if tokens.is_empty() {
        return Err(AlgebraError::Empty);
    }
    let mut parser = Parser { tokens, pos: 0 };
    let expr = parser.sum(0)?;
    match parser.peek() {
        None => {}
        Some(Tok::Bar) => return Err(AlgebraError::SlashBetweenFractions),
        Some(Tok::Close) => return Err(AlgebraError::UnexpectedClose),
        Some(_) => return Err(AlgebraError::MissingOperator),
    }
    checked(expr)
}

/// At least one and at most [`MAX_OPERATIONS`] operations.
fn checked<V: Value>(expr: Expr<V>) -> Result<Expr<V>, AlgebraError> {
    match pending(&expr).len() {
        0 => Err(AlgebraError::MissingOperator),
        n if n > MAX_OPERATIONS => Err(AlgebraError::TooLarge),
        _ => Ok(expr),
    }
}

/// An operation that still has to be calculated.
struct Pending<V> {
    /// Position in pre-order, which is also left-to-right for operations on two values.
    id: usize,
    /// Number of brackets around it.
    depth: usize,
    op: Operation,
    /// Both operands, if they are already values.
    operands: Option<(V, V)>,
}

fn pending<V: Value>(expr: &Expr<V>) -> Vec<Pending<V>> {
    fn collect<V: Value>(expr: &Expr<V>, depth: usize, out: &mut Vec<Pending<V>>) {
        match expr {
            Expr::Num(..) => {}
            Expr::Group(inner) => collect(inner, depth + 1, out),
            Expr::Neg(inner) => collect(inner, depth, out),
            Expr::Bin(left, op, right) => {
                let operands = match (&**left, &**right) {
                    (Expr::Num(a, _), Expr::Num(b, _)) => Some((a.clone(), b.clone())),
                    _ => None,
                };
                out.push(Pending { id: out.len(), depth, op: *op, operands });
                collect(left, depth, out);
                collect(right, depth, out);
            }
        }
    }
    let mut out = Vec::new();
    collect(expr, 0, &mut out);
    out
}

/// Innermost brackets first, then multiplication and division, then from left to right.
fn choose<V>(pending: &[Pending<V>]) -> Option<(&Pending<V>, &V, &V)> {
    pending
        .iter()
        .filter_map(|p| p.operands.as_ref().map(|(a, b)| (p, a, b)))
        .max_by_key(|(p, ..)| (p.depth, p.op.is_product(), Reverse(p.id)))
}

/// Rebuilds the calculation with the operation `id` (on two values) replaced by `f`'s result.
fn rewrite<V: Value>(expr: &Expr<V>, id: usize, f: &impl Fn(&Expr<V>) -> Expr<V>) -> Expr<V> {
    fn walk<V: Value>(expr: &Expr<V>, id: usize, next: &mut usize, f: &impl Fn(&Expr<V>) -> Expr<V>) -> Expr<V> {
        match expr {
            Expr::Num(..) => expr.clone(),
            Expr::Group(inner) => Expr::Group(Box::new(walk(inner, id, next, f))),
            Expr::Neg(inner) => Expr::Neg(Box::new(walk(inner, id, next, f))),
            Expr::Bin(left, op, right) => {
                let this = *next;
                *next += 1;
                // The target only has values below it, so skipping it doesn't shift later ids.
                if this == id {
                    return f(expr);
                }
                let left = walk(left, id, next, f);
                Expr::Bin(Box::new(left), *op, Box::new(walk(right, id, next, f)))
            }
        }
    }
    walk(expr, id, &mut 0, f)
}

/// What changed around a new intermediate result.
#[derive(Default)]
struct Tidied {
    bracket: bool,
    sign: bool,
}

/// Removes brackets around single values and applies a minus in front of them.
fn tidy<V: Value>(expr: Expr<V>, notes: &mut Tidied) -> Expr<V> {
    match expr {
        Expr::Num(..) => expr,
        Expr::Group(inner) => match tidy(*inner, notes) {
            num @ Expr::Num(..) => {
                notes.bracket = true;
                num
            }
            inner => Expr::Group(Box::new(inner)),
        },
        Expr::Neg(inner) => match tidy(*inner, notes) {
            num @ Expr::Num(..) => {
                notes.sign = true;
                negate(num)
            }
            inner => Expr::Neg(Box::new(inner)),
        },
        Expr::Bin(left, op, right) => Expr::Bin(Box::new(tidy(*left, notes)), op, Box::new(tidy(*right, notes))),
    }
}

fn starts_with_minus(line: &Line) -> bool {
    matches!(line.first().map(|t| &t.glyph), Some(Glyph::Text(text)) if text.trim_start().starts_with('−'))
}

/// Whether a value needs brackets: after an operator if it starts with a minus, and as a
/// sum where it would otherwise mix with the operator (`wrap`).
fn needs_brackets<V: Value>(value: &V, line: &Line, first: bool, wrap: bool) -> bool {
    (starts_with_minus(line) && !first) || (wrap && value.is_sum())
}

fn line<V: Value>(expr: &Expr<V>) -> Line {
    // `first`: nothing stands before it; `wrap`: a sum here needs brackets.
    fn draw<V: Value>(expr: &Expr<V>, first: bool, wrap: bool, line: &mut Line) {
        match expr {
            Expr::Num(value, marked) => {
                let own = value.line(marked.then_some(0));
                let bracket = needs_brackets(value, &own, first, wrap);
                if bracket {
                    push_text(line, "(", None, false);
                }
                line.extend(own);
                if bracket {
                    push_text(line, ")", None, false);
                }
            }
            Expr::Group(inner) => {
                push_text(line, "(", None, false);
                draw(inner, true, false, line);
                push_text(line, ")", None, false);
            }
            Expr::Neg(inner) => {
                push_text(line, "−", None, false);
                draw(inner, false, true, line);
            }
            Expr::Bin(left, op, right) => {
                let marked = matches!((&**left, &**right), (Expr::Num(_, true), Expr::Num(_, true)));
                draw(left, first, op.is_product(), line);
                push_text(line, format!(" {} ", op.symbol()), marked.then_some(0), false);
                draw(right, false, op.is_product() || *op == Operation::Subtract, line);
            }
        }
    }
    let mut out = Line::new();
    draw(expr, true, false, &mut out);
    out
}

fn value_text<V: Value>(value: &V, first: bool, wrap: bool) -> String {
    let text = value.text();
    if needs_brackets(value, &value.line(None), first, wrap) { format!("({text})") } else { text }
}

fn calculation_text<V: Value>(a: &V, op: Operation, b: &V) -> String {
    let right_wrap = op.is_product() || op == Operation::Subtract;
    format!("{} {} {}", value_text(a, true, op.is_product()), op.symbol(), value_text(b, false, right_wrap))
}

pub fn steps(input: &str) -> Result<Vec<Step>, AlgebraError> {
    if input.chars().any(char::is_alphabetic) {
        solve(symbolic::parse(input)?)
    } else {
        solve(parse(input)?)
    }
}

fn solve<V: Value>(mut expr: Expr<V>) -> Result<Vec<Step>, AlgebraError> {
    let original = line(&expr);
    let total = pending(&expr).len();
    let mut steps = Vec::new();
    if total > 1 {
        steps.push(order_step(&expr, total));
    }

    loop {
        let pending = pending(&expr);
        let Some((next, a, b)) = choose(&pending) else { break };
        let (id, op) = (next.id, next.op);
        let calculation = calculation_text(a, op, b);
        if total > 1 {
            let marked = rewrite(&expr, id, &|bin| match bin {
                Expr::Bin(_, op, _) => Expr::Bin(Box::new(Expr::Num(a.clone(), true)), *op, Box::new(Expr::Num(b.clone(), true))),
                _ => bin.clone(),
            });
            steps.push(Step::new(format!("Als Nächstes: {calculation}"), reasons(&pending, next), line(&marked)));
        }

        let value = V::calculate(a, op, b, &mut steps)?;
        let mut notes = Tidied::default();
        let shown = tidy(rewrite(&expr, id, &|_| Expr::Num(value.clone(), true)), &mut notes);
        expr = tidy(rewrite(&expr, id, &|_| Expr::Num(value.clone(), false)), &mut Tidied::default());
        if pending.len() > 1 {
            steps.push(insert_step(calculation, &value, &notes, line(&shown)));
        }
    }

    match expr {
        Expr::Num(value, _) => {
            steps.push(V::result_step(original, &value));
            Ok(steps)
        }
        _ => Err(AlgebraError::Malformed),
    }
}

fn order_step<V: Value>(expr: &Expr<V>, total: usize) -> Step {
    let ops = pending(expr);
    let mut rules = Vec::new();
    if ops.iter().any(|p| p.depth > 0) {
        rules.push("Klammern zuerst – bei Klammern in Klammern die innerste zuerst.");
    }
    if ops.iter().any(|p| p.op.is_product()) && ops.iter().any(|p| !p.op.is_product()) {
        rules.push("Punkt vor Strich: Mal (·) und Geteilt (:) kommen vor Plus (+) und Minus (−).");
    }
    rules.push("Sonst wird von links nach rechts gerechnet.");
    let rules: Vec<String> = rules.iter().enumerate().map(|(k, rule)| format!("{}. {rule}", k + 1)).collect();
    Step::new(
        "Reihenfolge festlegen",
        format!(
            "Diese Rechnung besteht aus {total} Teilrechnungen. Wir rechnen sie einzeln aus, in dieser Reihenfolge:\n{}\n\
             Warum? Nur mit festen Regeln hat jede Rechnung genau ein Ergebnis – egal, wer sie ausrechnet.",
            rules.join("\n")
        ),
        line(expr),
    )
}

/// Why `next` is calculated now.
fn reasons<V>(pending: &[Pending<V>], next: &Pending<V>) -> String {
    let marker = color_name(0);
    if pending.len() == 1 {
        return format!("Jetzt bleibt nur noch eine Rechnung übrig ({marker} markiert).");
    }
    let mut lines = Vec::new();
    if next.depth > 0 {
        let innermost = if next.depth > 1 { ", und zwar in der innersten" } else { "" };
        lines.push(format!(
            "Klammern zuerst: Diese Rechnung steht in einer Klammer{innermost} ({marker} markiert).\n\
             Warum? Klammern zeigen an, was zuerst ausgerechnet werden muss."
        ));
    }
    let same_level = |p: &&Pending<V>| p.id != next.id && p.depth == next.depth;
    if next.op.is_product() && pending.iter().filter(same_level).any(|p| !p.op.is_product()) {
        lines.push(
            "Punkt vor Strich: Mal (·) und Geteilt (:) werden vor Plus (+) und Minus (−) gerechnet.\n\
             Warum? Das ist eine feste Regel. Mal ist eine Abkürzung für mehrfaches Plus: 2 + 3 · 4 = 2 + 4 + 4 + 4 = 14."
                .to_string(),
        );
    }
    if pending.iter().filter(same_level).any(|p| p.op.is_product() == next.op.is_product()) {
        let example = if next.op.is_product() {
            "(8 : 4) : 2 = 1, aber 8 : (4 : 2) = 4"
        } else {
            "(8 − 4) − 2 = 2, aber 8 − (4 − 2) = 6"
        };
        lines.push(format!(
            "Von links nach rechts: Bei gleichrangigen Rechnungen kommt die linke zuerst.\n\
             Warum? Bei Minus und Geteilt hängt das Ergebnis von der Reihenfolge ab: {example}."
        ));
    }
    if lines.is_empty() {
        lines.push(format!("Diese Rechnung ist jetzt an der Reihe ({marker} markiert)."));
    }
    lines.join("\n")
}

fn insert_step<V: Value>(calculation: String, value: &V, notes: &Tidied, line: Line) -> Step {
    let mut lines = vec![format!(
        "{calculation} = {}. Das Teilergebnis ersetzt die Teilrechnung ({} markiert).",
        value_text(value, true, false),
        color_name(0)
    )];
    if notes.sign {
        lines.push(format!(
            "Das Minus vor der Klammer dreht das Vorzeichen um: −({}) = {}.",
            value.text(),
            value.negated().text()
        ));
    } else if notes.bracket {
        let kept = if starts_with_minus(&value.line(None)) || value.is_sum() {
            " Eine negative Zahl oder eine Summe nach einem Rechenzeichen bleibt aber eingeklammert."
        } else {
            ""
        };
        lines.push(format!("Die Klammer ist ausgerechnet und wird nicht mehr gebraucht.{kept}"));
    }
    lines.push("Warum? So wird die Rechnung mit jedem Schritt kürzer, bis nur noch ein Ergebnis übrig ist.".to_string());
    Step::new("Teilergebnis einsetzen", lines.join("\n"), line)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn result(input: &str) -> String {
        line_text(&steps(input).unwrap().last().unwrap().line)
    }

    fn lines(input: &str, title: &str) -> Vec<String> {
        steps(input).unwrap().iter().filter(|s| s.title.starts_with(title)).map(|s| line_text(&s.line)).collect()
    }

    #[test]
    fn calculates_in_the_right_order() {
        assert_eq!(result("1/2 + 1/3 · 3/4"), "1/2 + 1/3 · 3/4 = 3/4");
        assert_eq!(result("(1/2 + 1/3) · 3/5"), "(1/2 + 1/3) · 3/5 = 1/2");
        assert_eq!(result("3/4 - 1/2 : 2"), "3/4 − 1/2 : 2 = 1/2");
        assert_eq!(result("2/3 · (1/4 + 1/2) - 1/6"), "2/3 · (1/4 + 1/2) − 1/6 = 1/3");
        assert_eq!(result("(5/6 - 1/3) : (1/4 + 1/8)"), "(5/6 − 1/3) : (1/4 + 1/8) = 4/3");
        assert_eq!(result("1 1/2 + 3/4 · 2 - 1/3"), "11/2 + 3/4 · 2 − 1/3 = 8/3");
        assert_eq!(result("-(1/2 - 1/3) · 6"), "−(1/2 − 1/3) · 6 = −1");
        assert_eq!(result("1/2 - 1/4 - 1/8"), "1/2 − 1/4 − 1/8 = 1/8");
        assert_eq!(result("1 : 1/2 : 4"), "1 : 1/2 : 4 = 1/2");
        assert_eq!(result("2 · (1/2 - (1/3 + 1/4))"), "2 · (1/2 − (1/3 + 1/4)) = −1/6");
    }

    #[test]
    fn explains_each_operation_and_inserts_its_result() {
        assert_eq!(lines("1/2 + 1/3 · 3/4", "Als Nächstes"), ["1/2 + 1/3 · 3/4", "1/2 + 1/4"]);
        assert_eq!(lines("1/2 + 1/3 · 3/4", "Teilergebnis"), ["1/2 + 1/4"]);
        assert_eq!(lines("2 · (1/2 - (1/3 + 1/4))", "Teilergebnis"), ["2 · (1/2 − 7/12)", "2 · (−1/12)"]);

        let steps = steps("1/2 + 1/3 · 3/4").unwrap();
        assert_eq!(steps[0].title, "Reihenfolge festlegen");
        assert_eq!(steps[1].title, "Als Nächstes: 1/3 · 3/4");
        assert!(steps[1].explanation.contains("Punkt vor Strich"));
        let first = super::steps("(1/2 + 1/3) · 3/5").unwrap();
        assert!(first[1].explanation.contains("Klammern zuerst"));
        let left = super::steps("1/2 - 1/4 - 1/8").unwrap();
        assert!(left[1].explanation.contains("links nach rechts"));
        let sign = super::steps("-(1/2 - 1/3) · 6").unwrap();
        assert!(sign.iter().any(|s| s.explanation.contains("dreht das Vorzeichen um: −(1/6) = −1/6.")));
        let mixed = super::steps("1 1/2 · 2 + 1").unwrap();
        assert_eq!(mixed[1].title, "Als Nächstes: 1 1/2 · 2");
    }

    #[test]
    fn a_single_operation_needs_no_order() {
        let steps = steps("1/4 + 2/3").unwrap();
        assert_eq!(steps[0].title, "Hauptnenner bestimmen");
        assert_eq!(line_text(&steps.last().unwrap().line), "1/4 + 2/3 = 11/12");
    }

    #[test]
    fn reports_input_errors() {
        assert_eq!(parse("").unwrap_err(), AlgebraError::Empty);
        assert_eq!(parse("2/3").unwrap_err(), AlgebraError::MissingOperator);
        assert_eq!(parse("(2/3)").unwrap_err(), AlgebraError::MissingOperator);
        assert_eq!(parse("1/2 + 1/3 3/4").unwrap_err(), AlgebraError::MissingOperator);
        assert_eq!(parse("(1/2 + 1/3)/2").unwrap_err(), AlgebraError::SlashBetweenFractions);
        assert_eq!(parse("(1/2 + 1/3").unwrap_err(), AlgebraError::MissingClose);
        assert_eq!(parse("1/2 + 1/3)").unwrap_err(), AlgebraError::UnexpectedClose);
        assert_eq!(parse("1/2 + ").unwrap_err(), AlgebraError::UnexpectedEnd);
        assert_eq!(parse("1+1+1+1+1+1+1+1+1+1").unwrap_err(), AlgebraError::TooLarge);
        assert_eq!(parse("((((((((1+1))))))))").unwrap_err(), AlgebraError::TooDeep);
        assert_eq!(steps("1 : (1/2 - 1/2)").unwrap_err(), AlgebraError::DivisionByZero);
    }
}
