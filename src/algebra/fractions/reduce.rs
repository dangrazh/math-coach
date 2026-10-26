//! Cancelling fractions: numbers via their prime factors, variables written out as single
//! factors, or powers with the exponent rule.

use super::super::{color_name, AlgebraError, Line, Step};
use super::{equals, fraction_line, items_line, numbers_only, parse_fraction, written_fraction, Atom, Factored, Item, Part};

/// More single factors than this per side are too long to write out.
const MAX_ITEMS: usize = 40;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Style {
    /// Only numbers, cancelled via their prime factors.
    Numbers,
    /// Powers written out as single factors (`a² = a·a`), then cancelled.
    Variables,
    /// Equal bases cancelled with the rule `x⁵ : x³ = x⁵⁻³`.
    Powers,
}

pub fn steps(input: &str, style: Style) -> Result<Vec<Step>, AlgebraError> {
    let (top, bottom) = parse_fraction(input)?;
    if style == Style::Numbers {
        numbers_only(&[&top, &bottom])?;
    }
    let numerator = Factored::new(&top)?;
    let denominator = Factored::new(&bottom)?;
    if denominator.part.is_zero() {
        return Err(AlgebraError::DivisionByZero);
    }
    let original = written_fraction(&top, &bottom)?;
    let mut steps = Vec::new();
    if numerator.part.is_zero() {
        steps.push(Step::new(
            "Ergebnis",
            "Der Zähler ist 0. Null geteilt durch eine Zahl (ausser 0) ergibt immer 0.",
            equals(original, super::super::tree::text_line("0")),
        ));
        return Ok(steps);
    }

    steps.extend(factoring_step(&numerator, &denominator));
    let (num, den) = (&numerator.part, &denominator.part);
    if num.negative || den.negative {
        steps.push(sign_step(num, den));
    }
    let fraction = Fraction { negative: num.negative != den.negative, num: num.abs(), den: den.abs() };
    let common = fraction.num.common(&fraction.den);
    let cancelled = match style {
        Style::Numbers | Style::Variables => fraction.write_out_steps(style, &common, &mut steps)?,
        Style::Powers => fraction.power_steps(&common, &mut steps),
    };
    if cancelled {
        steps.push(fraction.result_step(original, &common, style));
    }
    Ok(steps)
}

/// "Kürzen darf man nur Faktoren": sums are factored first.
fn factoring_step(num: &Factored, den: &Factored) -> Option<Step> {
    if num.notes.is_empty() && den.notes.is_empty() {
        return None;
    }
    let mut lines =
        vec!["Kürzen darf man nur Faktoren, keine Summanden. Darum schreiben wir Zähler und Nenner zuerst als Produkte:".to_string()];
    lines.extend(num.notes.iter().map(|n| format!("Zähler: {n}")));
    lines.extend(den.notes.iter().map(|n| format!("Nenner: {n}")));
    let step = Step::new("Faktorisieren", lines.join("\n"), fraction_line(false, num.part.signed_line(), den.part.signed_line()));
    Some(match num.negative_content || den.negative_content {
        true => step.with_hint("Beim Ausklammern einer negativen Zahl kehren sich in der Klammer alle Vorzeichen um."),
        false => step,
    })
}

fn sign_step(num: &Part, den: &Part) -> Step {
    let negative = num.negative != den.negative;
    let mut line = fraction_line(negative, num.abs().sum().line(), den.abs().sum().line());
    if negative {
        line[0].sign = true;
    }
    let explanation = match (num.negative, den.negative) {
        (true, true) => "Im Zähler und im Nenner steht ein Minus. Minus geteilt durch Minus ergibt Plus – beide Minuszeichen fallen weg.",
        (true, false) => "Das Minus im Zähler schreiben wir vor den Bruch. Danach kürzen wir nur noch mit positiven Zahlen.",
        _ => "Ein Minus im Nenner darf man vor den Bruch schreiben, denn Plus geteilt durch Minus ergibt Minus.",
    };
    Step::new("Vorzeichen vor den Bruch", explanation, line)
}

/// Numbers split into prime factors, or into ggT and rest.
fn write_out(part: &Part, style: Style, ggt: i128) -> Vec<Item> {
    let numbers = match style {
        Style::Numbers => primes(part.number),
        _ if part.number == 1 => Vec::new(),
        _ if ggt > 1 && ggt < part.number => vec![ggt, part.number / ggt],
        _ => vec![part.number],
    };
    numbers
        .into_iter()
        .map(Atom::Number)
        .chain(part.vars.iter().flat_map(|(v, e)| (0..e).map(move |_| Atom::Var(v, 1))))
        .chain(part.polys.iter().flat_map(|(p, e)| (0..*e).map(move |_| Atom::Poly(p.clone(), 1))))
        .map(Item::new)
        .collect()
}

/// Every factor as a power of its base: `15·x⁷·y³`.
fn power_items(part: &Part) -> Vec<Item> {
    (part.number != 1)
        .then_some(Atom::Number(part.number))
        .into_iter()
        .chain(part.vars.iter().map(|(v, e)| Atom::Var(v, e)))
        .chain(part.polys.iter().map(|(p, e)| Atom::Poly(p.clone(), *e)))
        .map(Item::new)
        .collect()
}

fn primes(mut n: i128) -> Vec<i128> {
    let mut out = Vec::new();
    let mut p = 2;
    while p * p <= n {
        while n % p == 0 {
            out.push(p);
            n /= p;
        }
        p += 1;
    }
    if n > 1 {
        out.push(n);
    }
    out
}

/// Gives every factor that occurs above and below a colour, one colour per base, starting
/// at `first`. Returns the matched bases with how often each was matched.
fn mark_common(top: &mut [Item], bottom: &mut [Item], first: usize) -> Vec<(Atom, usize)> {
    let mut bases: Vec<(Atom, usize)> = Vec::new();
    let mut used = vec![false; bottom.len()];
    for item in top.iter_mut() {
        let partner = bottom.iter_mut().zip(used.iter_mut()).find(|(b, used)| !**used && b.atom.same_base(&item.atom));
        let Some((partner, used)) = partner else { continue };
        *used = true;
        let index = match bases.iter().position(|(b, _)| b.same_base(&item.atom)) {
            Some(index) => index,
            None => {
                bases.push((item.atom.clone(), 0));
                bases.len() - 1
            }
        };
        bases[index].1 += 1;
        item.color = Some(first + index);
        partner.color = Some(first + index);
    }
    bases
}

fn strike_marked(items: &mut [Item]) {
    items.iter_mut().filter(|i| i.color.is_some()).for_each(|i| i.struck = true);
}

/// Splits off a leading number item.
fn split_number(items: &mut [Item]) -> (&mut [Item], &mut [Item]) {
    let numbers = usize::from(matches!(items.first(), Some(Item { atom: Atom::Number(_), .. })));
    items.split_at_mut(numbers)
}

fn join(items: &[String]) -> String {
    items.join(", ")
}

/// A fraction with positive parts and its sign in front.
struct Fraction {
    negative: bool,
    num: Part,
    den: Part,
}

impl Fraction {
    fn line(&self, top: &[Item], bottom: &[Item]) -> Line {
        fraction_line(self.negative, items_line(top), items_line(bottom))
    }

    /// The fraction after dividing by `common`; a denominator of 1 is left out.
    fn result_line(&self, common: &Part) -> Line {
        let (num, den) = (self.num.without(common), self.den.without(common));
        if den.is_one() {
            Part { negative: self.negative, ..num }.signed_line()
        } else {
            fraction_line(self.negative, num.sum().line(), den.sum().line())
        }
    }

    fn not_reducible(&self, style: Style, line: Line) -> Step {
        let explanation = match style {
            Style::Numbers => format!(
                "{} und {} haben keinen gemeinsamen Primfaktor (ggT = 1). Der Bruch ist schon vollständig gekürzt.",
                self.num.number, self.den.number
            ),
            _ => "Zähler und Nenner haben keinen gemeinsamen Faktor. Der Bruch lässt sich nicht kürzen.".to_string(),
        };
        let step = Step::new("Nicht kürzbar", explanation, line);
        if self.num.polys.is_empty() && self.den.polys.is_empty() {
            step
        } else {
            step.with_hint(
                "Achtung: Aus Summen darf man nicht kürzen! Einzelne Summanden in einer Klammer darf man nicht streichen – \
                 nur ganze Klammern, die oben und unten als Faktor vorkommen.",
            )
        }
    }

    /// Decompose, mark, strike. Returns whether anything could be cancelled.
    fn write_out_steps(&self, style: Style, common: &Part, steps: &mut Vec<Step>) -> Result<bool, AlgebraError> {
        let ggt = common.number;
        let mut top = write_out(&self.num, style, ggt);
        let mut bottom = write_out(&self.den, style, ggt);
        if top.len() > MAX_ITEMS || bottom.len() > MAX_ITEMS {
            return Err(AlgebraError::TooLarge);
        }
        if let Some(explanation) = self.decomposition(style, ggt) {
            steps.push(Step::new("In Faktoren zerlegen", explanation, self.line(&top, &bottom)));
        }

        let bases = mark_common(&mut top, &mut bottom, 0);
        if bases.is_empty() {
            steps.push(self.not_reducible(style, self.line(&top, &bottom)));
            return Ok(false);
        }
        let names: Vec<String> = bases
            .iter()
            .enumerate()
            .map(|(i, (atom, count))| format!("{} ({})", vec![atom.text(); *count].join("·"), color_name(i)))
            .collect();
        let explanation = match style {
            Style::Numbers => format!(
                "Diese Primfaktoren kommen oben und unten vor: {}.\nZusammen ergeben sie den grössten gemeinsamen Teiler: ggT = {ggt}.",
                join(&names)
            ),
            _ => format!("Diese Faktoren kommen oben und unten vor: {}.", join(&names)),
        };
        steps.push(Step::new("Gemeinsame Faktoren markieren", explanation, self.line(&top, &bottom)));

        strike_marked(&mut top);
        strike_marked(&mut bottom);
        let explanation = match style {
            Style::Numbers => format!(
                "Jeden markierten Faktor oben und unten streichen. Das ist dasselbe, wie Zähler und Nenner durch den ggT {ggt} \
                 zu teilen: {} : {ggt} = {}, {} : {ggt} = {}.",
                self.num.number,
                self.num.number / ggt,
                self.den.number,
                self.den.number / ggt
            ),
            _ => format!(
                "Jeden markierten Faktor oben und unten streichen – das heisst, Zähler und Nenner durch {} teilen. \
                 Was übrig bleibt, wird wieder zusammengeschrieben.",
                match common.sum().0.len() {
                    1 => common.text(),
                    _ => format!("({})", common.text()),
                }
            ),
        };
        let line = equals(self.line(&top, &bottom), self.result_line(common));
        steps.push(Step::new("Kürzen", explanation, line).with_hint(self.leftover_hint(common).join(" ")));
        Ok(true)
    }

    /// What has to be written out; `None` if the factors are already single.
    fn decomposition(&self, style: Style, ggt: i128) -> Option<String> {
        let (n, d) = (self.num.number, self.den.number);
        let mut lines = Vec::new();
        if style == Style::Numbers {
            lines.push("Zähler und Nenner in Primfaktoren zerlegen:".to_string());
            for number in [n, d] {
                let factors: Vec<String> = primes(number).iter().map(i128::to_string).collect();
                lines.push(match factors.len() {
                    0 => format!("{number} hat keine Primfaktoren."),
                    1 => format!("{number} ist eine Primzahl."),
                    _ => format!("{number} = {}", factors.join("·")),
                });
            }
            return Some(lines.join("\n"));
        }
        lines.push("Zähler und Nenner als Produkt einzelner Faktoren schreiben:".to_string());
        if n > 1 && d > 1 {
            let split = |x: i128| if x == ggt { format!("{x} ist selbst der ggT") } else { format!("{x} = {ggt}·{}", x / ggt) };
            lines.push(match ggt {
                1 => format!("Zahlen: ggT({n}, {d}) = 1 – die Zahlen haben keinen gemeinsamen Teiler."),
                _ => format!("Zahlen: ggT({n}, {d}) = {ggt}, also {} und {}.", split(n), split(d)),
            });
        }
        let mut powers: Vec<String> = Vec::new();
        for part in [&self.num, &self.den] {
            for (v, e) in part.vars.iter().filter(|&(_, e)| e > 1) {
                let atom = Atom::Var(v, 1);
                powers.push(format!("{} = {}", atom.power_text(e), vec![atom.text(); e as usize].join("·")));
            }
            for (p, e) in part.polys.iter().filter(|(_, e)| *e > 1) {
                let atom = Atom::Poly(p.clone(), 1);
                powers.push(format!("{} = {}", atom.power_text(*e), vec![atom.text(); *e as usize].join("·")));
            }
        }
        if !powers.is_empty() {
            lines.push(format!("Potenzen ausschreiben: {}", join(&powers)));
        }
        (lines.len() > 1).then(|| lines.join("\n"))
    }

    /// Mark equal bases, then subtract exponents. Returns whether anything could be cancelled.
    fn power_steps(&self, common: &Part, steps: &mut Vec<Step>) -> bool {
        let ggt = common.number;
        let mut top = power_items(&self.num);
        let mut bottom = power_items(&self.den);
        let (top_number, top_rest) = split_number(&mut top);
        let (bottom_number, bottom_rest) = split_number(&mut bottom);
        let numbers_match = ggt > 1;
        if numbers_match {
            top_number.iter_mut().chain(bottom_number.iter_mut()).for_each(|i| i.color = Some(0));
        }
        let bases = mark_common(top_rest, bottom_rest, usize::from(numbers_match));
        if !numbers_match && bases.is_empty() {
            steps.push(self.not_reducible(Style::Powers, self.line(&top, &bottom)));
            return false;
        }

        let mut lines = Vec::new();
        if numbers_match {
            lines.push(format!("Zahlen: ggT({}, {}) = {ggt} ({})", self.num.number, self.den.number, color_name(0)));
        }
        let exponent_of = |part: &Part, atom: &Atom| match atom {
            Atom::Var(v, _) => part.vars.exponent(*v),
            Atom::Poly(p, _) => part.polys.iter().find(|(q, _)| q.same(p)).map_or(0, |(_, e)| *e),
            _ => 0,
        };
        let color_offset = usize::from(numbers_match);
        for (i, (atom, _)) in bases.iter().enumerate() {
            let (m, n) = (exponent_of(&self.num, atom), exponent_of(&self.den, atom));
            lines.push(format!(
                "Basis {}: oben {}, unten {} ({})",
                atom.power_text(1),
                atom.power_text(m),
                atom.power_text(n),
                color_name(color_offset + i)
            ));
        }
        steps.push(Step::new(
            "Gleiche Basen markieren",
            format!("Gesucht sind Zahlen mit gemeinsamem Teiler und Potenzen mit gleicher Basis:\n{}", lines.join("\n")),
            self.line(&top, &bottom),
        ));

        let mut lines = Vec::new();
        if numbers_match {
            let (n, d) = (self.num.number, self.den.number);
            lines.push(format!("Zahlen durch den ggT teilen: {n} : {ggt} = {}, {d} : {ggt} = {}", n / ggt, d / ggt));
        }
        for (atom, _) in &bases {
            let (m, n) = (exponent_of(&self.num, atom), exponent_of(&self.den, atom));
            let (above, below) = (atom.power_text(m), atom.power_text(n));
            lines.push(match m.cmp(&n) {
                std::cmp::Ordering::Greater => format!(
                    "{above} : {below} = {}⁻{} = {} – bleibt oben, weil oben die grössere Hochzahl steht.",
                    above,
                    super::super::tree::superscript(n),
                    atom.power_text(m - n)
                ),
                std::cmp::Ordering::Less => format!(
                    "{above} : {below}: unten bleibt {}⁻{} = {}, weil unten die grössere Hochzahl steht.",
                    below,
                    super::super::tree::superscript(m),
                    atom.power_text(n - m)
                ),
                std::cmp::Ordering::Equal => format!("{above} : {below} = 1 – fällt ganz weg."),
            });
        }
        let subtract = |items: &[Item], other: &Part, own: &Part| -> Vec<Item> {
            items
                .iter()
                .filter_map(|item| match &item.atom {
                    Atom::Number(n) if item.color.is_some() => (n / ggt != 1).then(|| Item { atom: Atom::Number(n / ggt), ..item.clone() }),
                    atom @ (Atom::Var(..) | Atom::Poly(..)) if item.color.is_some() => {
                        let (mine, theirs) = (exponent_of(own, atom), exponent_of(other, atom));
                        (mine > theirs).then(|| Item { minus: Some(theirs), ..item.clone() })
                    }
                    _ => Some(item.clone()),
                })
                .collect()
        };
        let top = subtract(&top, &self.den, &self.num);
        let bottom = subtract(&bottom, &self.num, &self.den);
        let mut hints = vec!["Potenzen mit gleicher Basis dividiert man, indem man die Hochzahlen subtrahiert, z. B. x⁵ : x³ = x⁵⁻³ = x²."];
        hints.extend(self.leftover_hint(common));
        steps.push(Step::new("Hochzahlen subtrahieren", lines.join("\n"), self.line(&top, &bottom)).with_hint(hints.join(" ")));
        true
    }

    /// Warnings about a numerator or denominator that is cancelled completely.
    fn leftover_hint(&self, common: &Part) -> Vec<&'static str> {
        let mut hints = Vec::new();
        if self.num.without(common).is_one() {
            hints.push("Fällt im Zähler alles weg, bleibt dort eine 1 stehen – nicht 0!");
        }
        if self.den.without(common).is_one() {
            hints.push("Steht im Nenner nur noch 1, lässt man den Bruchstrich weg.");
        }
        hints
    }

    fn result_step(&self, original: Line, common: &Part, style: Style) -> Step {
        let (num, den) = (self.num.without(common), self.den.without(common));
        let step = Step::new("Ergebnis", "Der Bruch ist vollständig gekürzt.", equals(original, self.result_line(common)));
        let plain = style == Style::Numbers;
        if plain && !den.is_one() && num.number > den.number {
            let sign = if self.negative { "−" } else { "" };
            step.with_hint(format!(
                "Der Zähler ist grösser als der Nenner (unechter Bruch). Als gemischte Zahl: {sign}{} {}/{}.",
                num.number / den.number,
                num.number % den.number,
                den.number
            ))
        } else {
            step
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::tree::line_text;
    use super::*;

    fn result(input: &str, style: Style) -> String {
        line_text(&steps(input, style).unwrap().last().unwrap().line)
    }

    fn titles(input: &str, style: Style) -> Vec<String> {
        steps(input, style).unwrap().into_iter().map(|s| s.title).collect()
    }

    #[test]
    fn cancels_numbers_via_prime_factors() {
        assert_eq!(result("12/18", Style::Numbers), "12/18 = 2/3");
        assert_eq!(titles("12/18", Style::Numbers), ["In Faktoren zerlegen", "Gemeinsame Faktoren markieren", "Kürzen", "Ergebnis"]);
        assert_eq!(result("36/12", Style::Numbers), "36/12 = 3");
        assert_eq!(result("-14/21", Style::Numbers), "−14/21 = −2/3");
        assert_eq!(result("12/(-18)", Style::Numbers), "12/−18 = −2/3");
        assert_eq!(result("0/5", Style::Numbers), "0/5 = 0");
        let steps = steps("13/20", Style::Numbers).unwrap();
        assert_eq!(steps.last().unwrap().title, "Nicht kürzbar");
        assert_eq!(super::steps("x/2", Style::Numbers).unwrap_err(), AlgebraError::VariablesNotAllowed);
        assert_eq!(super::steps("3/0", Style::Numbers).unwrap_err(), AlgebraError::DivisionByZero);
    }

    #[test]
    fn strikes_the_common_factors() {
        let steps = steps("12/18", Style::Numbers).unwrap();
        let cancel = steps.iter().find(|s| s.title == "Kürzen").unwrap();
        let super::super::super::Glyph::Stacked(top, _) = &cancel.line[0].glyph else { panic!("no fraction") };
        let struck: Vec<&str> = top
            .iter()
            .filter(|t| t.struck)
            .map(|t| match &t.glyph {
                super::super::super::Glyph::Text(s) => s.as_str(),
                _ => "",
            })
            .collect();
        assert_eq!(struck, ["2", "3"]);
        let mixed = steps.last().unwrap();
        assert!(mixed.hint.is_empty());
        assert!(super::steps("14/6", Style::Numbers).unwrap().last().unwrap().hint.contains("2 1/3"));
    }

    #[test]
    fn cancels_variables_and_brackets() {
        assert_eq!(result("12a²b/18ab³", Style::Variables), "12a²b/18ab³ = 2a/3b²");
        assert_eq!(result("6xy/9x", Style::Variables), "6xy/9x = 2y/3");
        assert_eq!(result("(4x + 8)/(6x + 12)", Style::Variables), "(4x + 8)/(6x + 12) = 2/3");
        assert_eq!(result("(x² - 9)/(x + 3)", Style::Variables), "(x² − 9)/(x + 3) = x − 3");
        assert_eq!(result("(a² + 2ab + b²)/(3a + 3b)", Style::Variables), "(a² + 2ab + b²)/(3a + 3b) = (a + b)/3");
        assert_eq!(result("(2x - 6)/(9 - 3x)", Style::Variables), "(2x − 6)/(9 − 3x) = −2/3");
        assert_eq!(titles("(x + 2)/2", Style::Variables).last().unwrap(), "Nicht kürzbar");
        assert_eq!(titles("(4x + 8)/(6x + 12)", Style::Variables)[0], "Faktorisieren");
    }

    #[test]
    fn cancels_powers_by_subtracting_exponents() {
        assert_eq!(result("x⁵/x³", Style::Powers), "x⁵/x³ = x²");
        assert_eq!(result("a⁴b²/(a²b⁵)", Style::Powers), "a⁴b²/a²b⁵ = a²/b³");
        assert_eq!(result("15x⁷y³/(25x⁴y⁵)", Style::Powers), "15x⁷y³/25x⁴y⁵ = 3x³/5y²");
        assert_eq!(result("-12m³n⁶/(18m³n²)", Style::Powers), "−12m³n⁶/18m³n² = −2n⁴/3");
        assert_eq!(result("(x + 1)⁵/(x + 1)²", Style::Powers), "((x + 1)⁵)/((x + 1)²) = (x + 1)³");
        assert_eq!(result("8a²b/(12a⁴b)", Style::Powers), "8a²b/12a⁴b = 2/3a²");
        let steps = steps("15x⁷y³/(25x⁴y⁵)", Style::Powers).unwrap();
        let rule = steps.iter().find(|s| s.title == "Hochzahlen subtrahieren").unwrap();
        assert_eq!(line_text(&rule.line), "(3·x⁷⁻⁴)/(5·y⁵⁻³)");
    }
}
