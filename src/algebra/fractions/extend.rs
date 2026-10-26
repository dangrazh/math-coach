//! Expanding fractions: numerator and denominator are multiplied by the same factor,
//! given directly ("mit 5") or through the new denominator ("auf 20").

use super::super::parse::{self, Expr};
use super::super::tree::{line_text, Factor, Group, Mono, Sum, Summand};
use super::super::{AlgebraError, Line, Step};
use super::{equals, fraction_line, items_line, normalized, numbers_only, parse_fraction, terms, terms_text, written_fraction, Atom, Factored, Item};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Keyword {
    /// "mit": the factor itself.
    With,
    /// "auf": the new denominator.
    To,
}

/// Position of `word` as a word of its own (case-insensitive).
fn find_word(input: &str, word: &str) -> Option<usize> {
    input.char_indices().map(|(i, _)| i).find(|&i| {
        let before = input[..i].chars().next_back().is_none_or(|c| !c.is_alphabetic());
        let matches = input.get(i..i + word.len()).is_some_and(|w| w.eq_ignore_ascii_case(word));
        let after = input.get(i + word.len()..).is_some_and(|rest| rest.chars().next().is_none_or(|c| !c.is_alphabetic()));
        before && matches && after
    })
}

/// Splits `3/4 mit 5` into the fraction, the keyword and its argument.
fn split_keyword(input: &str) -> Result<(&str, Keyword, &str), AlgebraError> {
    [("mit", Keyword::With), ("auf", Keyword::To)]
        .into_iter()
        .find_map(|(word, keyword)| find_word(input, word).map(|at| (&input[..at], keyword, &input[at + word.len()..])))
        .filter(|(_, _, argument)| !argument.trim().is_empty())
        .ok_or(AlgebraError::MissingExtension)
}

/// The term as a single factor of a product.
fn factor_of(terms: &[Mono]) -> Factor {
    match terms {
        [mono] => Factor::Mono(mono.clone()),
        _ => Factor::Group(Group { sum: Sum::from_terms(terms.to_vec()), power: 1 }),
    }
}

pub fn steps(input: &str, variables: bool) -> Result<Vec<Step>, AlgebraError> {
    if input.trim().is_empty() {
        return Err(AlgebraError::Empty);
    }
    let (fraction, keyword, argument) = split_keyword(input)?;
    let (top, bottom) = parse_fraction(fraction)?;
    let argument = parse::expression(argument)?;
    if !variables {
        numbers_only(&[&top, &bottom, &argument])?;
    }
    let num = terms(&top)?;
    let den = terms(&bottom)?;
    if den.is_empty() {
        return Err(AlgebraError::DivisionByZero);
    }

    let mut steps = Vec::new();
    let factor = match keyword {
        Keyword::With => {
            let factor = terms(&argument)?;
            if factor.is_empty() {
                return Err(AlgebraError::ExtendByZero);
            }
            factor
        }
        Keyword::To => {
            let (factor, step) = find_factor(&bottom, &den, &argument, variables)?;
            steps.push(step);
            factor
        }
    };
    let negative = matches!(factor.as_slice(), [m] if m.coeff.is_negative());
    let (what, noun) = if variables { ("Ausmultiplizieren", "demselben Term") } else { ("Ausrechnen", "derselben Zahl") };
    let factor_text = terms_text(&factor);

    let marked = Item { sign: negative, ..Item::new(Atom::of_terms(&factor)).colored(0) };
    let top_items = [Item::new(Atom::of_terms(&num)), marked.clone()];
    let bottom_items = [Item::new(Atom::of_terms(&den)), marked];
    let one = match factor.len() {
        1 => format!("{factor_text}/{factor_text}"),
        _ => format!("({factor_text})/({factor_text})"),
    };
    let step = Step::new(
        "Zähler und Nenner multiplizieren",
        format!(
            "Beim Erweitern werden Zähler und Nenner mit {noun} multipliziert – hier mit {factor_text} (blau). \
             Der Wert des Bruchs bleibt gleich, weil {one} = 1 ist."
        ),
        fraction_line(false, items_line(&top_items), items_line(&bottom_items)),
    );
    steps.push(match negative {
        true => step.with_hint(
            "Wir erweitern mit einer negativen Zahl: Zähler und Nenner wechseln beide ihr Vorzeichen. Der Wert bleibt trotzdem gleich.",
        ),
        false => step,
    });

    let product = |side: &[Mono]| -> Result<Vec<Mono>, AlgebraError> {
        if side.is_empty() {
            return Ok(Vec::new());
        }
        normalized(Sum(vec![Summand::new(vec![factor_of(side), factor_of(&factor)])]))
    };
    let (new_num, new_den) = (product(&num)?, product(&den)?);
    let explanation = format!(
        "Zähler: {} = {}\nNenner: {} = {}",
        line_text(&items_line(&top_items)),
        terms_text(&new_num),
        line_text(&items_line(&bottom_items)),
        terms_text(&new_den)
    );
    let hint = if [&num, &den, &factor].iter().any(|t| t.len() > 1) {
        "Klammern ausmultiplizieren: Jeder Term der einen Klammer wird mit jedem Term der anderen multipliziert."
    } else if factor.iter().any(|f| [&num, &den].iter().any(|side| side.iter().any(|t| !t.vars.common(&f.vars).is_empty()))) {
        "Gleiche Variablen multipliziert man, indem man die Hochzahlen addiert, z. B. a·a = a² oder b·b² = b³."
    } else {
        ""
    };
    let result = || fraction_line(false, Sum::from_terms(new_num.clone()).line(), Sum::from_terms(new_den.clone()).line());
    steps.push(Step::new(what, explanation, result()).with_hint(hint));

    let summary = match variables {
        true => "Der erweiterte Bruch hat denselben Wert wie der ursprüngliche.",
        false => "Der erweiterte Bruch hat denselben Wert wie der ursprüngliche – Zähler und Nenner sind nur grösser geworden.",
    };
    steps.push(Step::new("Ergebnis", summary, equals(written_fraction(&top, &bottom)?, result())));
    Ok(steps)
}

/// The factor that turns the denominator into the target: target ÷ denominator.
fn find_factor(bottom: &Expr, den: &[Mono], target_expr: &Expr, variables: bool) -> Result<(Vec<Mono>, Step), AlgebraError> {
    let target = terms(target_expr)?;
    let not_multiple = || AlgebraError::NotAMultiple(terms_text(&target), terms_text(den));
    let old = Factored::new(bottom)?;
    let new = Factored::new(target_expr)?;
    if new.part.is_zero() {
        return Err(not_multiple());
    }
    let factor_part = new.part.divide(&old.part).ok_or_else(not_multiple)?;
    let factor = factor_part.expand()?;
    let (d, t, f) = (terms_text(den), terms_text(&target), terms_text(&factor));

    let explanation = match (den, target.as_slice(), factor.as_slice()) {
        ([_], [_], [_]) if !variables => {
            format!("Mit welcher Zahl muss man den Nenner {d} multiplizieren, damit {t} herauskommt?\n{t} : {d} = {f}")
        }
        ([dm], [tm], [fm]) => {
            let mut lines = vec![format!("Neuer Nenner geteilt durch alten Nenner: {t} : {d} = {f}")];
            if !(tm.coeff.is_one() && dm.coeff.is_one()) {
                lines.push(format!("Zahlen: {} : {} = {}", tm.coeff, dm.coeff, fm.coeff));
            }
            for (v, e) in tm.vars.iter() {
                let below = dm.vars.exponent(v);
                let power = |e: u32| Atom::Var(v, 1).power_text(e);
                lines.push(match below {
                    0 => format!("{v}: kommt nur im neuen Nenner vor → {}", power(e)),
                    _ if below == e => format!("{v}: {} : {} = 1", power(e), power(below)),
                    _ => format!("{v}: {} : {} = {}", power(e), power(below), power(e - below)),
                });
            }
            lines.join("\n")
        }
        _ => {
            let mut lines = vec![format!("Der neue Nenner muss den alten Nenner {d} als Faktor enthalten.")];
            lines.extend(old.notes.iter().map(|n| format!("Alter Nenner: {n}")));
            lines.extend(new.notes.iter().map(|n| format!("Neuer Nenner: {n}")));
            lines.push(format!("Es fehlt noch der Faktor {f}."));
            lines.join("\n")
        }
    };
    let negative = matches!(factor.as_slice(), [m] if m.coeff.is_negative());
    let items = [Item::new(Atom::of_terms(den)), Item { sign: negative, ..Item::new(Atom::of_terms(&factor)).colored(0) }];
    let line: Line = equals(items_line(&items), Sum::from_terms(target.clone()).line());
    let title = if variables { "Erweiterungsfaktor bestimmen" } else { "Erweiterungszahl bestimmen" };
    let step = Step::new(title, explanation, line);
    let step = match negative {
        true => step.with_hint("Der neue Nenner hat das andere Vorzeichen, darum ist die Erweiterungszahl negativ."),
        false => step,
    };
    Ok((factor, step))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn result(input: &str, variables: bool) -> String {
        line_text(&steps(input, variables).unwrap().last().unwrap().line)
    }

    #[test]
    fn expands_numbers() {
        assert_eq!(result("3/4 mit 5", false), "3/4 = 15/20");
        assert_eq!(result("2/3 auf 12", false), "2/3 = 8/12");
        assert_eq!(result("-1/6 auf 24", false), "−1/6 = −4/24");
        let steps = steps("2/3 auf 12", false).unwrap();
        assert_eq!(steps[0].title, "Erweiterungszahl bestimmen");
        assert_eq!(line_text(&steps[0].line), "3·4 = 12");
        assert_eq!(line_text(&steps[1].line), "(2·4)/(3·4)");
    }

    #[test]
    fn rejects_impossible_expansions() {
        assert_eq!(steps("3/4", false).unwrap_err(), AlgebraError::MissingExtension);
        assert_eq!(steps("3/4 mit 0", false).unwrap_err(), AlgebraError::ExtendByZero);
        assert_eq!(steps("3/4 auf 10", false).unwrap_err(), AlgebraError::NotAMultiple("10".into(), "4".into()));
        assert_eq!(steps("3/4 mit a", false).unwrap_err(), AlgebraError::VariablesNotAllowed);
    }

    #[test]
    fn expands_with_variables() {
        assert_eq!(result("2a/3b mit 4a", true), "2a/3b = 8a²/12ab");
        assert_eq!(result("x/y auf xy²", true), "x/y = x²y/xy²");
        assert_eq!(result("3/(4x) auf 12x²y", true), "3/4x = 9xy/12x²y");
        assert_eq!(result("x/(x + 1) mit (x - 1)", true), "x/(x + 1) = (x² − x)/(x² − 1)");
        assert_eq!(result("1/(x - 2) auf x² - 4", true), "1/(x − 2) = (x + 2)/(x² − 4)");
        assert!(matches!(steps("1/(x - 2) auf x² + 4", true), Err(AlgebraError::NotAMultiple(..))));
    }
}
