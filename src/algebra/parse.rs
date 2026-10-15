//! Tokenizer and recursive-descent parser for terms and equations.
//!
//! Grammar (implicit multiplication binds like `·`):
//! ```text
//! sum     := term (('+' | '-') term)*
//! term    := ('+' | '-') term | product
//! product := power (('·' | ':') signed | implicit power)*
//! signed  := ('+' | '-') signed | power
//! power   := atom ('^' integer | superscript)?
//! atom    := number | variable | '(' sum ')'
//! ```

use std::collections::BTreeSet;

use super::rational::Rational;
use super::AlgebraError;

const MAX_INPUT: usize = 200;
const MAX_DIGITS: usize = 7;
const MAX_EXPONENT: i128 = 9;
const MAX_DEPTH: usize = 12;

/// A parsed term, close to how it was written.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Num(Rational),
    Var(char),
    Neg(Box<Expr>),
    Sum(Vec<Expr>),
    /// Factors with the operator written before each (ignored for the first).
    Product(Vec<(Op, Expr)>),
    Power(Box<Expr>, u32),
    Paren(Box<Expr>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    /// Written without operator, as in `3x` or `2(a + b)`.
    Implicit,
    Times,
    Divide,
}

impl Expr {
    pub fn collect_variables(&self, out: &mut BTreeSet<char>) {
        match self {
            Expr::Num(_) => {}
            Expr::Var(v) => {
                out.insert(*v);
            }
            Expr::Neg(inner) | Expr::Power(inner, _) | Expr::Paren(inner) => inner.collect_variables(out),
            Expr::Sum(terms) => terms.iter().for_each(|t| t.collect_variables(out)),
            Expr::Product(items) => items.iter().for_each(|(_, e)| e.collect_variables(out)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Tok {
    Num(Rational),
    Var(char),
    Plus,
    Minus,
    Times,
    Divide,
    Caret,
    Exponent(u32),
    Open,
    Close,
    Equals,
}

fn superscript_value(c: char) -> Option<u32> {
    "⁰¹²³⁴⁵⁶⁷⁸⁹".chars().position(|s| s == c).map(|d| d as u32)
}

fn lex(input: &str) -> Result<Vec<Tok>, AlgebraError> {
    if input.chars().count() > MAX_INPUT {
        return Err(AlgebraError::TooLong);
    }
    let mut tokens = Vec::new();
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        let token = match c {
            _ if c.is_whitespace() => continue,
            '0'..='9' => {
                let mut digits = String::from(c);
                let mut decimals: Option<u32> = None;
                while let Some(&next) = chars.peek() {
                    match next {
                        '0'..='9' => {
                            digits.push(next);
                            if let Some(d) = decimals.as_mut() {
                                *d += 1;
                            }
                        }
                        '.' | ',' if decimals.is_none() => decimals = Some(0),
                        _ => break,
                    }
                    chars.next();
                }
                if digits.len() > MAX_DIGITS {
                    return Err(AlgebraError::NumberTooLarge);
                }
                let value: i128 = digits.parse().map_err(|_| AlgebraError::NumberTooLarge)?;
                Tok::Num(Rational::new(value, 10_i128.pow(decimals.unwrap_or(0))))
            }
            _ if c.is_ascii_alphabetic() => Tok::Var(c),
            '+' => Tok::Plus,
            '-' | '−' | '–' => Tok::Minus,
            '*' | '·' | '×' | '⋅' | '•' => Tok::Times,
            '/' | ':' | '÷' => Tok::Divide,
            '^' => Tok::Caret,
            '(' | '[' | '{' => Tok::Open,
            ')' | ']' | '}' => Tok::Close,
            '=' => Tok::Equals,
            _ => match superscript_value(c) {
                Some(e) => Tok::Exponent(e),
                None => return Err(AlgebraError::UnexpectedChar(c)),
            },
        };
        tokens.push(token);
    }
    Ok(tokens)
}

struct Parser {
    tokens: Vec<Tok>,
    pos: usize,
    depth: usize,
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

    fn sum(&mut self) -> Result<Expr, AlgebraError> {
        let mut terms = vec![self.term()?];
        while let Some(op @ (Tok::Plus | Tok::Minus)) = self.peek() {
            self.pos += 1;
            let term = self.term()?;
            terms.push(if op == Tok::Minus { Expr::Neg(Box::new(term)) } else { term });
        }
        Ok(if terms.len() == 1 { terms.remove(0) } else { Expr::Sum(terms) })
    }

    /// A sign at the start of a term applies to the whole product: `-3x` is `-(3x)`.
    fn term(&mut self) -> Result<Expr, AlgebraError> {
        match self.peek() {
            Some(Tok::Minus) => {
                self.pos += 1;
                Ok(Expr::Neg(Box::new(self.term()?)))
            }
            Some(Tok::Plus) => {
                self.pos += 1;
                self.term()
            }
            _ => self.product(),
        }
    }

    fn product(&mut self) -> Result<Expr, AlgebraError> {
        let mut items = vec![(Op::Implicit, self.power()?)];
        loop {
            let (op, factor) = match self.peek() {
                Some(Tok::Times) => {
                    self.pos += 1;
                    (Op::Times, self.signed()?)
                }
                Some(Tok::Divide) => {
                    self.pos += 1;
                    (Op::Divide, self.signed()?)
                }
                Some(Tok::Var(_) | Tok::Open) => (Op::Implicit, self.power()?),
                _ => break,
            };
            items.push((op, factor));
        }
        Ok(if items.len() == 1 { items.remove(0).1 } else { Expr::Product(items) })
    }

    /// A factor after `·` or `:` may carry its own sign, as in `2·-3`.
    fn signed(&mut self) -> Result<Expr, AlgebraError> {
        match self.peek() {
            Some(Tok::Minus) => {
                self.pos += 1;
                Ok(Expr::Neg(Box::new(self.signed()?)))
            }
            Some(Tok::Plus) => {
                self.pos += 1;
                self.signed()
            }
            _ => self.power(),
        }
    }

    fn power(&mut self) -> Result<Expr, AlgebraError> {
        let base = self.atom()?;
        let exponent = match self.peek() {
            Some(Tok::Caret) => {
                self.pos += 1;
                match self.next() {
                    Some(Tok::Num(n)) if n.is_integer() => n.numer(),
                    _ => return Err(AlgebraError::InvalidExponent),
                }
            }
            Some(Tok::Exponent(e)) => {
                self.pos += 1;
                e.into()
            }
            _ => return Ok(base),
        };
        if exponent > MAX_EXPONENT {
            return Err(AlgebraError::ExponentTooLarge);
        }
        Ok(Expr::Power(Box::new(base), exponent as u32))
    }

    fn atom(&mut self) -> Result<Expr, AlgebraError> {
        match self.next() {
            Some(Tok::Num(n)) => Ok(Expr::Num(n)),
            Some(Tok::Var(v)) => Ok(Expr::Var(v)),
            Some(Tok::Open) => {
                self.depth += 1;
                if self.depth > MAX_DEPTH {
                    return Err(AlgebraError::TooDeep);
                }
                let inner = self.sum()?;
                self.depth -= 1;
                match self.next() {
                    Some(Tok::Close) => Ok(Expr::Paren(Box::new(inner))),
                    _ => Err(AlgebraError::MissingClose),
                }
            }
            Some(Tok::Close) => Err(AlgebraError::UnexpectedClose),
            Some(_) => Err(AlgebraError::MissingOperand),
            None => Err(AlgebraError::UnexpectedEnd),
        }
    }
}

fn parse_tokens(tokens: &[Tok]) -> Result<Expr, AlgebraError> {
    if tokens.is_empty() {
        return Err(AlgebraError::Empty);
    }
    let mut parser = Parser { tokens: tokens.to_vec(), pos: 0, depth: 0 };
    let expr = parser.sum()?;
    match parser.peek() {
        None => Ok(expr),
        Some(Tok::Close) => Err(AlgebraError::UnexpectedClose),
        Some(Tok::Num(_)) => Err(AlgebraError::MisplacedNumber),
        Some(_) => Err(AlgebraError::Malformed),
    }
}

/// Parses a term (no `=` allowed).
pub fn expression(input: &str) -> Result<Expr, AlgebraError> {
    let tokens = lex(input)?;
    if tokens.contains(&Tok::Equals) {
        return Err(AlgebraError::UnexpectedEquals);
    }
    parse_tokens(&tokens)
}

/// Parses an equation `left = right`.
pub fn equation(input: &str) -> Result<(Expr, Expr), AlgebraError> {
    let tokens = lex(input)?;
    if tokens.is_empty() {
        return Err(AlgebraError::Empty);
    }
    let mut sides = tokens.split(|t| *t == Tok::Equals);
    match (sides.next(), sides.next(), sides.next()) {
        (Some(_), None, _) => Err(AlgebraError::MissingEquals),
        (Some(_), Some(_), Some(_)) => Err(AlgebraError::TooManyEquals),
        (Some(left), Some(right), None) if !left.is_empty() && !right.is_empty() => {
            Ok((parse_tokens(left)?, parse_tokens(right)?))
        }
        _ => Err(AlgebraError::EmptySide),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leading_minus_covers_the_whole_product() {
        let expr = expression("-3x").unwrap();
        let three_x = Expr::Product(vec![(Op::Implicit, Expr::Num(Rational::int(3))), (Op::Implicit, Expr::Var('x'))]);
        assert_eq!(expr, Expr::Neg(Box::new(three_x)));
    }

    #[test]
    fn reads_decimals_and_superscripts() {
        assert_eq!(expression("2,5").unwrap(), Expr::Num(Rational::new(5, 2)));
        assert_eq!(expression("x²").unwrap(), Expr::Power(Box::new(Expr::Var('x')), 2));
        assert_eq!(expression("x^3").unwrap(), Expr::Power(Box::new(Expr::Var('x')), 3));
    }

    #[test]
    fn reports_errors() {
        assert_eq!(expression(""), Err(AlgebraError::Empty));
        assert_eq!(expression("3x +"), Err(AlgebraError::UnexpectedEnd));
        assert_eq!(expression("(x + 1"), Err(AlgebraError::MissingClose));
        assert_eq!(expression("x + 1)"), Err(AlgebraError::UnexpectedClose));
        assert_eq!(expression("x2"), Err(AlgebraError::MisplacedNumber));
        assert_eq!(expression("3 $ 4"), Err(AlgebraError::UnexpectedChar('$')));
        assert_eq!(expression("x = 2"), Err(AlgebraError::UnexpectedEquals));
        assert_eq!(equation("x + 2").unwrap_err(), AlgebraError::MissingEquals);
        assert_eq!(equation("x = 2 = 3").unwrap_err(), AlgebraError::TooManyEquals);
        assert_eq!(equation("= 2").unwrap_err(), AlgebraError::EmptySide);
    }
}
