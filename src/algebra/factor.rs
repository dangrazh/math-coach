//! Factoring out the greatest common factor.

use super::rational::{gcd, lcm, Rational};
use super::simplify;
use super::tree::{push_text, Factor, Group, Mark, Mono, Sum, Summand, Vars};
use super::{parse, AlgebraError, Step};

pub fn steps(input: &str) -> Result<Vec<Step>, AlgebraError> {
    let (stages, terms) = simplify::normalize(Sum::from_expr(&parse::expression(input)?)?)?;
    let mut steps: Vec<Step> = stages.into_iter().map(Step::from).collect();
    let original = Sum::from_terms(terms.clone());

    if terms.len() < 2 {
        steps.push(Step::new("Ergebnis", "Es gibt nur einen Term – da gibt es nichts auszuklammern.", original.line()));
        return Ok(steps);
    }
    let factor = common_factor(&terms);
    if factor.is_constant() && factor.coeff.is_one() {
        steps.push(Step::new(
            "Kein gemeinsamer Faktor",
            "Die Zahlen haben keinen gemeinsamen Teiler ausser 1, und keine Variable kommt in allen Termen vor. \
             Hier kann man nichts ausklammern.",
            original.line(),
        ));
        return Ok(steps);
    }
    let rests: Vec<Mono> = terms.iter().map(|t| t.div(&factor)).collect();
    let negative = factor.coeff.is_negative();
    let factor_mark = Mark::color(0);

    let decomposed = Sum(rests
        .iter()
        .enumerate()
        .map(|(i, rest)| {
            Summand::new(vec![
                Factor::Mono(factor.clone().marked(factor_mark)),
                Factor::Mono(rest.clone().marked(Mark::color(i + 1))),
            ])
        })
        .collect());
    steps.push(Step::new("Gemeinsamen Faktor suchen", explanation(&terms, &factor), decomposed.line()));

    let factored = |marks: bool| {
        let inner = rests
            .iter()
            .enumerate()
            .map(|(i, rest)| {
                let mark = if marks { Mark { color: Some(i + 1), sign: negative } } else { Mark::default() };
                Summand::mono(rest.clone()).marked(mark)
            })
            .collect();
        let lead = factor.clone().marked(if marks { factor_mark } else { Mark::default() });
        Sum(vec![Summand::new(vec![Factor::Mono(lead), Factor::Group(Group { sum: Sum(inner), power: 1 })])])
    };
    let mut step = Step::new(
        "Ausklammern",
        format!(
            "Den gemeinsamen Faktor {f} vor die Klammer schreiben. In die Klammer kommt, was von jedem Term \
             übrig bleibt (der Term geteilt durch {f}).",
            f = factor.text()
        ),
        factored(true).line(),
    );
    if negative {
        let changes: Vec<String> =
            terms.iter().zip(&rests).map(|(t, r)| format!("{} → {}", t.signed_text(), r.signed_text())).collect();
        step.hint = format!(
            "Weil wir eine negative Zahl ausgeklammert haben, kehren sich in der Klammer alle Vorzeichen um: {}.",
            changes.join(", ")
        );
    }
    steps.push(step);

    let mut probe = factored(false).line();
    push_text(&mut probe, " = ", None, false);
    probe.extend(original.line());
    steps.push(Step::new(
        "Probe",
        format!(
            "Zur Kontrolle wieder ausmultiplizieren: {} mal jeden Term in der Klammer ergibt wieder {}. ✓",
            factor.text(),
            original.text()
        ),
        probe,
    ));
    steps.push(Step::new("Ergebnis", "Der gemeinsame Faktor ist ausgeklammert.", factored(false).line()));
    Ok(steps)
}

/// The largest factor common to all (non-zero) terms; negative if the first term is.
fn common_factor(terms: &[Mono]) -> Mono {
    let numerator = terms.iter().fold(0, |g, t| gcd(g, t.coeff.numer()));
    let denominator = terms.iter().fold(1, |l, t| lcm(l, t.coeff.denom()));
    let vars = terms.iter().skip(1).fold(terms[0].vars.clone(), |acc, t| acc.common(&t.vars));
    let coeff = Rational::new(numerator, denominator);
    let coeff = if terms[0].coeff.is_negative() { -coeff } else { coeff };
    Mono::new(coeff, vars)
}

fn explanation(terms: &[Mono], factor: &Mono) -> String {
    let coefficients: Vec<String> = terms.iter().map(|t| t.coeff.abs().to_string()).collect();
    let size = factor.coeff.abs();
    let mut lines = Vec::new();
    if terms.iter().all(|t| t.coeff.is_integer()) {
        lines.push(match size.is_one() {
            true => format!("Zahlen: ggT({}) = 1 – kein gemeinsamer Zahlenfaktor.", coefficients.join(", ")),
            false => format!("Zahlen: ggT({}) = {size}", coefficients.join(", ")),
        });
    } else {
        lines.push(format!(
            "Zahlen: Aus {} lässt sich {size} herausziehen (ggT der Zähler geteilt durch kgV der Nenner).",
            coefficients.join(", ")
        ));
    }
    if factor.vars.is_empty() {
        lines.push("Variablen: Keine Variable kommt in jedem Term vor.".into());
    }
    for (var, exponent) in factor.vars.iter() {
        lines.push(format!(
            "Variablen: {var} kommt in jedem Term vor, die kleinste Hochzahl ist {exponent} → {}",
            Vars::single(var, exponent).text()
        ));
    }
    lines.push(format!("Gemeinsamer Faktor: {}", Mono::new(size, factor.vars.clone()).text()));
    if factor.coeff.is_negative() {
        lines.push(format!("Der erste Term ist negativ, darum klammern wir {} aus.", factor.text()));
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::super::tree::line_text;
    use super::*;

    fn result(input: &str) -> String {
        line_text(&steps(input).unwrap().last().unwrap().line)
    }

    #[test]
    fn factors_out_number_and_variables() {
        assert_eq!(result("6x + 9"), "3(2x + 3)");
        assert_eq!(result("4x² - 8x"), "4x(x − 2)");
        assert_eq!(result("12a²b + 18ab² - 6ab"), "6ab(2a + 3b − 1)");
        assert_eq!(result("x³ - 2x² + x"), "x(x² − 2x + 1)");
        assert_eq!(result("1/2x + 1/4"), "1/4(2x + 1)");
    }

    #[test]
    fn negative_first_term_flips_the_signs_inside() {
        let steps = steps("-3a - 6b").unwrap();
        assert_eq!(line_text(&steps.last().unwrap().line), "−3(a + 2b)");
        let factored = steps.iter().find(|s| s.title == "Ausklammern").unwrap();
        assert!(factored.hint.contains("−3a → +a"));
        assert!(factored.line.iter().any(|t| t.sign));
    }

    #[test]
    fn reports_when_nothing_can_be_factored() {
        let steps = steps("2x + 3").unwrap();
        assert_eq!(steps.last().unwrap().title, "Kein gemeinsamer Faktor");
    }
}
