//! Simplifying a term step by step: products, brackets and like terms.

use std::collections::HashSet;

use super::rational::Rational;
use super::tree::{Factor, Group, Mark, Mono, Sum, Summand, Vars};
use super::{color_name, AlgebraError};

const MAX_STAGES: usize = 60;
const MAX_MONOS: usize = 150;

/// One step of the simplification: what was done, and the whole term afterwards.
pub struct Stage {
    pub title: String,
    pub explanation: String,
    pub hint: String,
    pub sum: Sum,
}

struct Note {
    title: String,
    explanation: String,
    hint: String,
}

impl Note {
    fn new(title: &str, explanation: impl Into<String>) -> Self {
        Self { title: title.into(), explanation: explanation.into(), hint: String::new() }
    }

    fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = hint.into();
        self
    }

    fn stage(self, sum: &Sum) -> Stage {
        Stage { title: self.title, explanation: self.explanation, hint: self.hint, sum: sum.clone() }
    }
}

/// Resolves all products and brackets, then combines like terms. Returns the steps
/// and the resulting terms in standard order.
pub fn normalize(mut sum: Sum) -> Result<(Vec<Stage>, Vec<Mono>), AlgebraError> {
    let mut stages = Vec::new();
    loop {
        if stages.len() > MAX_STAGES || sum.mono_count() > MAX_MONOS {
            return Err(AlgebraError::TooLarge);
        }
        sum.clear_marks();
        // Innermost work first: products, then like terms inside brackets, then brackets.
        let note = multiply(&mut sum).or_else(|| combine_in_brackets(&mut sum)).or_else(|| expand(&mut sum));
        match note {
            Some(note) => stages.push(note.stage(&sum)),
            None => break,
        }
    }
    // Now every summand is a single monomial.
    let terms: Vec<Mono> = sum
        .0
        .into_iter()
        .map(|s| {
            s.factors
                .into_iter()
                .filter_map(|f| match f {
                    Factor::Mono(m) => Some(m),
                    Factor::Group(_) => None,
                })
                .fold(Mono::constant(Rational::ONE), |acc, m| acc.mul(&m))
        })
        .collect();
    let terms = combine(terms, &mut stages);
    if terms.iter().any(|t| t.coeff.is_overflow()) {
        return Err(AlgebraError::NumbersTooLarge);
    }
    Ok((stages, terms))
}

/// Multiplies the monomial factors of every product, e.g. `3a·4b = 12ab`.
fn multiply(sum: &mut Sum) -> Option<Note> {
    let mut lines: Vec<String> = Vec::new();
    let (mut even, mut odd, mut adds_exponents) = (false, false, false);
    sum.visit_mut(&mut |summand| {
        let monos: Vec<Mono> = summand
            .factors
            .iter()
            .filter_map(|f| match f {
                Factor::Mono(m) => Some(m.clone()),
                Factor::Group(_) => None,
            })
            .collect();
        if monos.len() < 2 {
            return;
        }
        let negatives = monos.iter().filter(|m| m.coeff.is_negative()).count();
        let mut seen = HashSet::new();
        adds_exponents |= monos.iter().flat_map(|m| m.vars.iter()).any(|(v, _)| !seen.insert(v));
        let product = monos.iter().fold(Mono::constant(Rational::ONE), |acc, m| acc.mul(m));
        let before = Summand::new(monos.into_iter().map(Factor::Mono).collect()).text();
        lines.push(format!("{before} = {}", product.text()));
        match negatives {
            0 => {}
            n if n % 2 == 0 => even = true,
            _ => odd = true,
        }

        // The product takes the place of the first monomial.
        let mut product = Some(product.marked(Mark { color: Some(lines.len() - 1), sign: negatives > 0 }));
        let factors = std::mem::take(&mut summand.factors);
        summand.factors = factors
            .into_iter()
            .filter_map(|f| match f {
                Factor::Mono(_) => product.take().map(Factor::Mono),
                group => Some(group),
            })
            .collect();
    });
    if lines.is_empty() {
        return None;
    }

    let rule = if adds_exponents {
        "Zahlen mit Zahlen multiplizieren, bei gleichen Variablen die Hochzahlen addieren (x·x = x²)"
    } else {
        "Zahlen mit Zahlen multiplizieren, Variablen dahinter schreiben"
    };
    let hint = [(even, "Minus mal Minus ergibt Plus."), (odd, "Plus mal Minus ergibt Minus.")]
        .iter()
        .filter(|(applies, _)| *applies)
        .map(|(_, rule)| *rule)
        .collect::<Vec<_>>()
        .join(" ");
    Some(Note::new("Produkte ausrechnen", format!("{rule}:\n{}", lines.join("\n"))).with_hint(hint))
}

/// Combines like terms inside brackets before the brackets are resolved.
fn combine_in_brackets(sum: &mut Sum) -> Option<Note> {
    fn walk(sum: &mut Sum, nested: bool, lines: &mut Vec<String>, hints: &mut Vec<String>) {
        for summand in &mut sum.0 {
            for group in summand.groups_mut() {
                walk(&mut group.sum, true, lines, hints);
            }
        }
        if !nested {
            return;
        }
        if let Some(terms) = sum.terms() {
            let combination = combine_terms(&terms, false);
            if combination.combined {
                *sum = Sum::from_terms(combination.terms);
                lines.extend(combination.lines);
                hints.extend(combination.hints);
            }
        }
    }

    let (mut lines, mut hints) = (Vec::new(), Vec::new());
    walk(sum, false, &mut lines, &mut hints);
    if lines.is_empty() {
        return None;
    }
    let explanation = format!("Zuerst gleichartige Terme in der Klammer zusammenfassen:\n{}", lines.join("\n"));
    Some(Note::new("In der Klammer zusammenfassen", explanation).with_hint(hints.join("\n")))
}

/// Resolves one innermost product that contains brackets.
fn expand(sum: &mut Sum) -> Option<Note> {
    let (path, index) = find_target(sum)?;
    let parent = sum.at_path_mut(&path);
    let summand = parent.0.remove(index);
    let (note, replacement) = expand_summand(summand);
    parent.0.splice(index..index, replacement);
    Some(note)
}

/// The first summand whose brackets contain no further brackets, as the path to the
/// sum holding it and its index there.
fn find_target(sum: &Sum) -> Option<(Vec<(usize, usize)>, usize)> {
    for (i, summand) in sum.0.iter().enumerate() {
        let mut has_group = false;
        for (j, factor) in summand.factors.iter().enumerate() {
            if let Factor::Group(group) = factor {
                if !group.sum.is_flat() {
                    let (mut path, index) = find_target(&group.sum)?;
                    path.insert(0, (i, j));
                    return Some((path, index));
                }
                has_group = true;
            }
        }
        if has_group {
            return Some((Vec::new(), i));
        }
    }
    None
}

fn expand_summand(summand: Summand) -> (Note, Vec<Summand>) {
    if summand.groups().any(|g| g.power != 1) {
        return write_out_powers(summand);
    }
    if summand.groups().any(|g| g.sum.0.len() <= 1) {
        let factors = summand
            .factors
            .into_iter()
            .flat_map(|f| match f {
                Factor::Group(g) => g.into_factors(),
                mono => vec![mono],
            })
            .collect();
        let note = Note::new("Klammer weglassen", "In der Klammer steht nur noch ein einziger Term – die Klammer braucht es nicht mehr.");
        return (note, vec![Summand::new(factors)]);
    }

    let (mut monos, mut groups) = (Vec::new(), Vec::new());
    for factor in summand.factors {
        match factor {
            Factor::Mono(m) => monos.push(m),
            Factor::Group(g) => groups.push(g),
        }
    }
    // A plain `+1` in front of a bracket is no factor at all.
    let factor = monos.into_iter().reduce(|a, b| a.mul(&b)).filter(|m| !(m.is_constant() && m.coeff.is_one()));
    let mut groups = groups.into_iter();
    let first = groups.next().expect("expansion targets contain a bracket");
    let Some(second) = groups.next() else { return distribute(factor, first) };

    let products: Vec<Summand> = first
        .sum
        .0
        .iter()
        .enumerate()
        .flat_map(|(i, t)| {
            second.sum.0.iter().map(move |u| Summand {
                factors: [t.factors.clone(), u.factors.clone()].concat(),
                mark: Mark::color(i),
            })
        })
        .collect();
    let note = Note::new(
        "Klammern ausmultiplizieren",
        "Jeder Term der ersten Klammer wird mit jedem Term der zweiten Klammer multipliziert. \
         Gleiche Farbe = gleicher Term aus der ersten Klammer.",
    );
    let rest: Vec<Factor> = groups.map(Factor::Group).collect();
    if factor.is_none() && rest.is_empty() {
        return (note, products);
    }
    let mut factors: Vec<Factor> = factor.into_iter().map(Factor::Mono).collect();
    factors.push(Factor::Group(Group { sum: Sum(products), power: 1 }));
    factors.extend(rest);
    (note, vec![Summand::new(factors)])
}

/// `(a + b)² = (a + b)(a + b)`.
fn write_out_powers(summand: Summand) -> (Note, Vec<Summand>) {
    let mut lines = Vec::new();
    let mut binomial = false;
    let mut factors = Vec::new();
    for factor in summand.factors {
        match factor {
            Factor::Group(group) if group.power != 1 => {
                let written = Summand::new(vec![Factor::Group(group.clone())]).text();
                binomial |= group.power == 2 && group.sum.0.len() == 2;
                let base = Group { sum: group.sum, power: 1 };
                let copies: Vec<Factor> = (0..group.power).flat_map(|_| base.clone().into_factors()).collect();
                let copies = if copies.is_empty() { vec![Factor::Mono(Mono::constant(Rational::ONE))] } else { copies };
                lines.push(format!("{written} = {}", Summand::new(copies.clone()).text()));
                factors.extend(copies);
            }
            other => factors.push(other),
        }
    }
    let explanation = format!("Die Hochzahl sagt, wie oft der Faktor mit sich selbst multipliziert wird:\n{}", lines.join("\n"));
    let mut note = Note::new("Potenz ausschreiben", explanation);
    if binomial {
        note.hint = "Tipp: Für (a + b)² gibt es eine Abkürzung, die binomische Formel (a + b)² = a² + 2ab + b². \
                     Hier rechnen wir Schritt für Schritt."
            .into();
    }
    (note, vec![Summand::new(factors)])
}

/// Multiplies a factor (or a sign) into a single bracket.
fn distribute(factor: Option<Mono>, group: Group) -> (Note, Vec<Summand>) {
    match factor {
        None => {
            let note = Note::new(
                "Klammer auflösen",
                "Vor der Klammer steht ein Plus (oder gar nichts). Die Klammer fällt einfach weg, \
                 alle Vorzeichen bleiben, wie sie sind.",
            );
            (note, group.sum.0)
        }
        Some(m) if m.is_constant() && m.coeff == -Rational::ONE => {
            let mut changes = Vec::new();
            let summands = group
                .sum
                .0
                .into_iter()
                .enumerate()
                .map(|(i, mut summand)| {
                    let before = signed_text(&summand);
                    summand.flip();
                    changes.push(format!("{before} wird zu {}", signed_text(&summand)));
                    summand.marked(Mark { color: Some(i), sign: true })
                })
                .collect();
            let explanation = format!(
                "Vor der Klammer steht ein Minus. Die Klammer fällt weg, dabei kehrt sich jedes Vorzeichen \
                 in der Klammer um:\n{}",
                changes.join(", ")
            );
            let hint = "Minus vor der Klammer: Aus + wird −, aus − wird +. \
                        Auch der erste Term in der Klammer hat ein (unsichtbares) Plus!";
            (Note::new("Minus vor der Klammer", explanation).with_hint(hint), summands)
        }
        Some(m) => {
            let negative = m.coeff.is_negative();
            let lead = m.clone().marked(Mark { color: None, sign: negative });
            let summands = group
                .sum
                .0
                .into_iter()
                .enumerate()
                .map(|(i, summand)| {
                    let mut factors = vec![Factor::Mono(lead.clone())];
                    factors.extend(summand.factors);
                    Summand { factors, mark: Mark::color(i) }
                })
                .collect();
            let text = m.text();
            let explanation = format!("Jeden Term in der Klammer mit {text} multiplizieren (jede Farbe ist ein Produkt).");
            let mut note = Note::new("Ausmultiplizieren", explanation);
            if negative {
                note.hint = format!(
                    "Das Minus gehört zum Faktor: Jeder Term wird mit {text} multipliziert, nicht mit {}.",
                    m.neg().text()
                );
            }
            (note, summands)
        }
    }
}

/// A term of a bracket with explicit sign, e.g. `+x` or `−3`.
fn signed_text(summand: &Summand) -> String {
    match summand.as_mono() {
        Some(m) => m.signed_text(),
        None => format!("+{}", summand.text()),
    }
}

/// The result of combining like terms.
#[derive(Default)]
pub struct Combination {
    pub terms: Vec<Mono>,
    /// One calculation per combined class, e.g. `3x − 5x = −2x`.
    pub lines: Vec<String>,
    /// Explanations for results whose sign differs from the first term's.
    pub hints: Vec<String>,
    pub combined: bool,
}

/// Groups like terms in order of first appearance (or standard order with `sort`).
fn classes(terms: &[Mono], sort: bool) -> Vec<(Vars, Vec<&Mono>)> {
    let mut classes: Vec<(Vars, Vec<&Mono>)> = Vec::new();
    for term in terms {
        match classes.iter_mut().find(|(vars, _)| *vars == term.vars) {
            Some((_, members)) => members.push(term),
            None => classes.push((term.vars.clone(), vec![term])),
        }
    }
    if sort {
        classes.sort_by(|a, b| a.0.standard_cmp(&b.0));
    }
    classes
}

/// Adds up like terms; results of combined classes are coloured in class order and
/// zero terms disappear.
pub fn combine_terms(terms: &[Mono], sort: bool) -> Combination {
    let mut out = Combination::default();
    let mut color = 0;
    for (vars, members) in classes(terms, sort) {
        let total = members.iter().fold(Rational::ZERO, |acc, m| acc + m.coeff);
        let mut result = Mono::new(total, vars);
        if members.len() > 1 {
            out.combined = true;
            let written = Sum::from_terms(members.iter().map(|m| Mono::new(m.coeff, m.vars.clone())).collect()).text();
            let sign_changed = !total.is_zero() && total.is_negative() != members[0].coeff.is_negative();
            if total.is_zero() {
                out.lines.push(format!("{written} = 0 (fällt weg)"));
            } else {
                out.lines.push(format!("{written} = {}", result.text()));
            }
            if sign_changed {
                out.hints.push(format!("{written}: {}", sign_reason(&members, total)));
            }
            result.mark = Mark { color: Some(color), sign: sign_changed };
            color += 1;
        }
        if !total.is_zero() {
            out.terms.push(result);
        }
    }
    out
}

fn sign_reason(members: &[&Mono], total: Rational) -> String {
    let (plus, minus) = members.iter().fold((Rational::ZERO, Rational::ZERO), |(plus, minus), m| {
        if m.coeff.is_negative() { (plus, minus - m.coeff) } else { (plus + m.coeff, minus) }
    });
    if total.is_negative() {
        format!("Es wird mehr abgezogen ({minus}) als dazugezählt ({plus}), darum ist das Ergebnis negativ.")
    } else {
        format!("Es wird mehr dazugezählt ({plus}) als abgezogen ({minus}), darum ist das Ergebnis positiv.")
    }
}

fn class_name(vars: &Vars) -> String {
    if vars.is_empty() { "Zahlen".into() } else { format!("{}-Terme", vars.text()) }
}

/// Combines like terms of a flat sum in three visible steps: mark, reorder, add up.
fn combine(terms: Vec<Mono>, stages: &mut Vec<Stage>) -> Vec<Mono> {
    let classes = classes(&terms, true);
    let palette: Vec<Vars> = classes.iter().filter(|(_, m)| m.len() > 1).map(|(v, _)| v.clone()).collect();
    if palette.is_empty() {
        return terms.into_iter().filter(|t| !t.coeff.is_zero()).collect();
    }
    let color_of = |vars: &Vars| palette.iter().position(|v| v == vars);

    let marked: Vec<Mono> = terms.iter().map(|t| t.clone().marked(Mark { color: color_of(&t.vars), sign: false })).collect();
    let names: Vec<String> = palette.iter().enumerate().map(|(i, v)| format!("{} ({})", class_name(v), color_name(i))).collect();
    let mut explanation = format!(
        "Gleichartige Terme haben dieselben Variablen mit denselben Hochzahlen. \
         Terme mit derselben Farbe können zusammengefasst werden: {}.",
        names.join(", ")
    );
    let singles: Vec<String> = classes.iter().filter(|(_, m)| m.len() == 1).map(|(_, m)| m[0].text()).collect();
    match singles.len() {
        0 => {}
        1 => explanation += &format!("\nOhne Partner: {} – dieser Term bleibt, wie er ist.", singles[0]),
        _ => explanation += &format!("\nOhne Partner: {} – diese Terme bleiben, wie sie sind.", singles.join(", ")),
    }
    stages.push(Stage { title: "Gleichartige Terme markieren".into(), explanation, hint: String::new(), sum: Sum::from_terms(marked.clone()) });

    let mut order: Vec<usize> = (0..terms.len()).collect();
    order.sort_by(|&a, &b| terms[a].vars.standard_cmp(&terms[b].vars));
    let sorted: Vec<Mono> = order.iter().map(|&i| marked[i].clone()).collect();
    if order.iter().enumerate().any(|(position, &i)| position != i) {
        let moved = order
            .iter()
            .enumerate()
            .map(|(position, &i)| {
                let mut term = marked[i].clone();
                term.mark.sign = position != i;
                term
            })
            .collect();
        stages.push(Stage {
            title: "Umordnen".into(),
            explanation: "Gleichartige Terme nebeneinander schreiben: höchste Hochzahl zuerst, Zahlen am Schluss.".into(),
            hint: "Jeder Term nimmt beim Verschieben sein Vorzeichen mit! Steht ein Term ganz vorne, \
                   wird sein Plus nicht geschrieben."
                .into(),
            sum: Sum::from_terms(moved),
        });
    }

    let combination = combine_terms(&sorted, false);
    stages.push(Stage {
        title: "Zusammenfassen".into(),
        explanation: format!(
            "Die Zahlen vor gleichartigen Termen zusammenrechnen, die Variablen bleiben gleich:\n{}",
            combination.lines.join("\n")
        ),
        hint: combination.hints.join("\n"),
        sum: Sum::from_terms(combination.terms.clone()),
    });
    combination.terms.into_iter().map(|t| t.marked(Mark::default())).collect()
}

#[cfg(test)]
mod tests {
    use super::super::parse;
    use super::*;

    fn run(input: &str) -> (Vec<Stage>, String) {
        let (stages, terms) = normalize(Sum::from_expr(&parse::expression(input).unwrap()).unwrap()).unwrap();
        (stages, Sum::from_terms(terms).text())
    }

    fn titles(stages: &[Stage]) -> Vec<&str> {
        stages.iter().map(|s| s.title.as_str()).collect()
    }

    #[test]
    fn combines_like_terms_in_three_steps() {
        let (stages, result) = run("3x + 2y - 5x + 4 - y + 7");
        assert_eq!(result, "−2x + y + 11");
        assert_eq!(titles(&stages), ["Gleichartige Terme markieren", "Umordnen", "Zusammenfassen"]);
        assert_eq!(stages[1].sum.text(), "3x − 5x + 2y − y + 4 + 7");
        assert!(stages[2].explanation.contains("3x − 5x = −2x"));
    }

    #[test]
    fn minus_before_a_bracket_flips_every_sign() {
        let (stages, result) = run("-3m + 4n - (2m - 5n)");
        assert_eq!(result, "−5m + 9n");
        assert_eq!(stages[0].title, "Minus vor der Klammer");
        assert_eq!(stages[0].sum.text(), "−3m + 4n − 2m + 5n");
        assert!(stages[0].sum.line().iter().any(|t| t.sign));
    }

    #[test]
    fn expands_products_of_brackets() {
        let (stages, result) = run("(x + 3)(x - 2)");
        assert_eq!(result, "x² + x − 6");
        assert_eq!(stages[0].sum.text(), "x·x + x·(−2) + 3·x + 3·(−2)");
        assert_eq!(stages[1].sum.text(), "x² − 2x + 3x − 6");
        assert_eq!(stages[1].hint, "Plus mal Minus ergibt Minus.");
        // −2x + 3x turns positive.
        assert!(!stages.last().unwrap().hint.is_empty());

        assert_eq!(run("(a + b)^2").1, "a² + 2ab + b²");
        assert_eq!(run("-2(a - 4b)").1, "−2a + 8b");
        assert_eq!(run("x(2x - 1) - 3(x + 4)").1, "2x² − 4x − 12");
        assert_eq!(run("2(x + 1)(x - 1)").1, "2x² − 2");
        assert_eq!(run("2(3(x + 1) - x)").1, "4x + 6");
        assert_eq!(run("(2x)^3").1, "8x³");
    }

    #[test]
    fn multiplies_monomials_with_sign_rules() {
        let (stages, result) = run("3a·4b - 2a·(-5a)");
        assert_eq!(result, "12ab + 10a²");
        assert_eq!(stages[0].title, "Produkte ausrechnen");
        assert!(stages[0].hint.contains("Minus mal Minus"));
        assert_eq!(run("-(-3)").1, "3");
    }

    #[test]
    fn huge_terms_are_rejected() {
        let input = "(99999x + 99999)^9";
        let result = normalize(Sum::from_expr(&parse::expression(input).unwrap()).unwrap());
        assert!(result.is_err());
    }
}
