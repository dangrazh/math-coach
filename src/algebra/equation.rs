//! Solving linear and quadratic equations in one variable, step by step.

use std::collections::BTreeSet;

use super::rational::{lcm, Rational};
use super::simplify::{self, Stage};
use super::tree::{push_number, push_text, text_line, Factor, Group, Mark, Mono, Sum, Summand, Vars};
use super::{parse, AlgebraError, Glyph, Line, Step, Token};

pub fn steps(input: &str) -> Result<Vec<Step>, AlgebraError> {
    let (left_expr, right_expr) = parse::equation(input)?;
    let mut variables = BTreeSet::new();
    left_expr.collect_variables(&mut variables);
    right_expr.collect_variables(&mut variables);
    if variables.len() > 1 {
        let names: Vec<String> = variables.iter().map(char::to_string).collect();
        return Err(AlgebraError::MultipleVariables(names.join(", ")));
    }

    let mut steps = Vec::new();
    let right_start = Sum::from_expr(&right_expr)?;
    let (stages, left) = simplify::normalize(Sum::from_expr(&left_expr)?)?;
    steps.extend(stages.into_iter().map(|s| side_step("Linke Seite", s, |sum| equation_line(sum, &right_start))));
    let left_done = Sum::from_terms(left.clone());
    let (stages, right) = simplify::normalize(right_start.clone())?;
    steps.extend(stages.into_iter().map(|s| side_step("Rechte Seite", s, |sum| equation_line(&left_done, sum))));

    let Some(&var) = variables.first() else {
        steps.push(verdict(constant_part(&left), constant_part(&right)));
        return Ok(steps);
    };
    let mut equation = Equation { left, right, var };
    if equation.degree() > 2 {
        return Err(AlgebraError::DegreeTooHigh);
    }
    let original = (equation.left.clone(), equation.right.clone());
    equation.clear_fractions(&mut steps);

    let roots = if equation.coefficient(2).is_zero() || equation.coefficient(1).is_zero() {
        match equation.isolate(&mut steps) {
            Isolated::Constants(left, right) => {
                steps.push(verdict(left, right));
                return Ok(steps);
            }
            Isolated::Power { degree: 1, value } => vec![Root::exact(value)],
            Isolated::Power { value, .. } => equation.square_root(&mut steps, value),
        }
    } else {
        equation.quadratic(&mut steps)
    };
    if roots.iter().any(|r| r.exact.is_some_and(Rational::is_overflow)) {
        return Err(AlgebraError::NumbersTooLarge);
    }
    steps.push(equation.solution_set(&roots));
    for value in roots.iter().filter_map(|r| r.exact) {
        steps.push(equation.probe(&original, value));
    }
    Ok(steps)
}

fn side_step(side: &str, stage: Stage, line: impl Fn(&Sum) -> Line) -> Step {
    Step { title: format!("{side}: {}", stage.title), line: line(&stage.sum), ..Step::new("", stage.explanation, Line::new()) }
        .with_hint(stage.hint)
}

fn equation_line(left: &Sum, right: &Sum) -> Line {
    let mut line = left.line();
    push_text(&mut line, " = ", None, false);
    line.extend(right.line());
    line
}

fn constant_part(terms: &[Mono]) -> Rational {
    terms.iter().filter(|t| t.is_constant()).fold(Rational::ZERO, |acc, t| acc + t.coeff)
}

fn plain(terms: &[Mono]) -> Vec<Mono> {
    terms.iter().map(|t| t.clone().marked(Mark::default())).collect()
}

/// `(−3)` for negative numbers, so that they can follow an operator.
fn paren(value: Rational) -> String {
    if value.is_negative() { format!("({value})") } else { value.to_string() }
}

fn approx(value: f64) -> String {
    format!("{value:.2}").replace('-', "−")
}

/// "a", "a und b", "a, b und c".
fn join_and(items: Vec<String>) -> String {
    match items.split_last() {
        Some((last, rest)) if !rest.is_empty() => format!("{} und {last}", rest.join(", ")),
        _ => items.concat(),
    }
}

fn verdict(left: Rational, right: Rational) -> Step {
    let line = text_line(format!("{left} = {right}"));
    if left == right {
        Step::new(
            "Immer wahr",
            format!("Es ist keine Variable mehr übrig, und {left} = {right} stimmt immer. Jede Zahl ist eine Lösung: L = ℚ."),
            line,
        )
    } else {
        Step::new(
            "Widerspruch",
            format!("Es ist keine Variable mehr übrig, und {left} = {right} ist falsch. Es gibt keine Lösung: L = {{ }}."),
            line,
        )
    }
}

/// A solution: exact, or an irrational square root shown with an approximation.
struct Root {
    exact: Option<Rational>,
    text: String,
    approx: f64,
}

impl Root {
    fn exact(value: Rational) -> Self {
        Self { exact: Some(value), text: value.to_string(), approx: value.to_f64() }
    }

    fn irrational(text: String, approx: f64) -> Self {
        Self { exact: None, text, approx }
    }

    fn push(&self, line: &mut Line, with_approx: bool) {
        match self.exact {
            Some(value) => push_number(line, value, None, false),
            None if with_approx => push_text(line, format!("{} ≈ {}", self.text, approx(self.approx)), None, false),
            None => push_text(line, self.text.clone(), None, false),
        }
    }
}

enum Isolated {
    /// The variable cancelled out, leaving `left = right`.
    Constants(Rational, Rational),
    /// `var^degree = value`.
    Power { degree: u32, value: Rational },
}

struct Equation {
    left: Vec<Mono>,
    right: Vec<Mono>,
    var: char,
}

impl Equation {
    fn line(&self) -> Line {
        equation_line(&Sum::from_terms(self.left.clone()), &Sum::from_terms(self.right.clone()))
    }

    fn degree(&self) -> u32 {
        self.left.iter().chain(&self.right).map(|t| t.vars.degree()).max().unwrap_or(0)
    }

    /// Coefficient of `var^degree` in `left − right`.
    fn coefficient(&self, degree: u32) -> Rational {
        let sum = |terms: &[Mono]| {
            terms.iter().filter(|t| t.vars.degree() == degree).fold(Rational::ZERO, |acc, t| acc + t.coeff)
        };
        sum(&self.left) - sum(&self.right)
    }

    fn power(&self, exponent: u32) -> Mono {
        Mono::power(self.var, exponent)
    }

    fn clear_fractions(&mut self, steps: &mut Vec<Step>) {
        let denominators: BTreeSet<i128> =
            self.left.iter().chain(&self.right).map(|t| t.coeff.denom()).filter(|&d| d > 1).collect();
        if denominators.is_empty() {
            return;
        }
        let factor = denominators.iter().fold(1, |l, &d| lcm(l, d));
        let scale = |terms: &[Mono]| -> Vec<Mono> {
            terms.iter().map(|t| Mono::new(t.coeff * Rational::int(factor), t.vars.clone())).collect()
        };
        self.left = scale(&self.left);
        self.right = scale(&self.right);
        let list: Vec<String> = denominators.iter().map(i128::to_string).collect();
        let explanation = format!(
            "In der Gleichung stehen Brüche mit den Nennern {}. Der Hauptnenner (kgV) ist {factor}. \
             Wir multiplizieren beide Seiten – jeden einzelnen Term! – mit {factor}, dann verschwinden die Brüche.",
            join_and(list)
        );
        steps.push(Step::new("Brüche wegschaffen", explanation, self.line()).with_operation(format!("| ·{factor}")));
    }

    /// Brings all variable terms to the left and all numbers to the right, then
    /// divides by the coefficient.
    fn isolate(&mut self, steps: &mut Vec<Step>) -> Isolated {
        let has_var = |terms: &[Mono]| terms.iter().any(|t| !t.is_constant());
        if !has_var(&self.left) && has_var(&self.right) {
            std::mem::swap(&mut self.left, &mut self.right);
            let explanation = format!(
                "{} kommt nur auf der rechten Seite vor. Wir vertauschen die beiden Seiten – das darf man, \
                 denn links und rechts steht gleich viel.",
                self.var
            );
            steps.push(Step::new("Seiten vertauschen", explanation, self.line()).with_operation("| Seiten tauschen"));
        }
        let movers: Vec<Mono> = self.right.iter().filter(|t| !t.is_constant()).cloned().collect();
        if !movers.is_empty() {
            self.move_terms(steps, &movers, true, &format!("{}-Terme", self.var));
        }
        if has_var(&self.left) {
            let movers: Vec<Mono> = self.left.iter().filter(|t| t.is_constant()).cloned().collect();
            if !movers.is_empty() {
                self.move_terms(steps, &movers, false, "Zahlen");
            }
        }
        match self.left.iter().find(|t| !t.is_constant()) {
            None => Isolated::Constants(constant_part(&self.left), constant_part(&self.right)),
            Some(term) => {
                let (coefficient, degree) = (term.coeff, term.vars.degree());
                let value = constant_part(&self.right);
                self.divide(steps, coefficient, degree, value);
                Isolated::Power { degree, value: value / coefficient }
            }
        }
    }

    /// Subtracts `movers` on both sides, then combines like terms.
    fn move_terms(&mut self, steps: &mut Vec<Step>, movers: &[Mono], to_left: bool, what: &str) {
        let mut classes: Vec<&Vars> = Vec::new();
        for mover in movers {
            if !classes.contains(&&mover.vars) {
                classes.push(&mover.vars);
            }
        }
        let color_of = |t: &Mono| classes.iter().position(|v| **v == t.vars);
        let marked = |terms: &[Mono]| -> Vec<Mono> {
            terms.iter().map(|t| t.clone().marked(Mark { color: color_of(t), sign: false })).collect()
        };
        let negated: Vec<Mono> = movers.iter().map(|m| m.neg().marked(Mark { color: color_of(m), sign: true })).collect();
        let left = [marked(&self.left), negated.clone()].concat();
        let right = [marked(&self.right), negated.clone()].concat();

        let (from, to) = if to_left { ("rechts", "links") } else { ("links", "rechts") };
        let operation: Vec<String> = negated.iter().map(Mono::signed_text).collect();
        let actions: Vec<String> = movers
            .iter()
            .map(|m| match m.coeff.is_negative() {
                true => format!("{} addieren", m.neg().text()),
                false => format!("{} subtrahieren", m.text()),
            })
            .collect();
        let explanation = format!(
            "Alle {what} sollen {to} stehen. Dazu auf beiden Seiten {} – so verschwindet {} {from}.",
            join_and(actions),
            join_and(movers.iter().map(Mono::text).collect())
        );
        let changes: Vec<String> = movers
            .iter()
            .map(|m| format!("aus {} {from} wird {} {to}", m.signed_text(), m.neg().signed_text()))
            .collect();
        let hint = format!("Seitenwechsel: {} – das Vorzeichen kehrt sich um!", changes.join(", "));
        let line = equation_line(&Sum::from_terms(left.clone()), &Sum::from_terms(right.clone()));
        steps.push(
            Step::new(format!("{what} nach {to} bringen"), explanation, line)
                .with_hint(hint)
                .with_operation(format!("| {}", operation.join(" "))),
        );
        self.combine(steps, &left, &right);
    }

    fn combine(&mut self, steps: &mut Vec<Step>, left: &[Mono], right: &[Mono]) {
        let left = simplify::combine_terms(&plain(left), true);
        let right = simplify::combine_terms(&plain(right), true);
        let mut lines = Vec::new();
        for (side, combination) in [("Links", &left), ("Rechts", &right)] {
            if !combination.lines.is_empty() {
                lines.push(format!("{side}: {}", combination.lines.join("; ")));
            }
        }
        if !lines.is_empty() {
            let line = equation_line(&Sum::from_terms(left.terms.clone()), &Sum::from_terms(right.terms.clone()));
            let explanation = format!("Gleichartige Terme auf jeder Seite zusammenfassen:\n{}", lines.join("\n"));
            let hints = [left.hints, right.hints].concat().join("\n");
            steps.push(Step::new("Zusammenfassen", explanation, line).with_hint(hints));
        }
        self.left = plain(&left.terms);
        self.right = plain(&right.terms);
    }

    /// `coefficient · var^degree = value` → `var^degree = value / coefficient`.
    fn divide(&mut self, steps: &mut Vec<Step>, coefficient: Rational, degree: u32, value: Rational) {
        if coefficient.is_one() {
            return;
        }
        let result = value / coefficient;
        let flips = coefficient.is_negative() && !value.is_zero();
        self.left = vec![self.power(degree)];
        self.right = vec![Mono::constant(result).marked(Mark { color: None, sign: flips })];
        let target = self.power(degree).text();
        let (title, operation, explanation) = if coefficient == -Rational::ONE {
            (
                "Mit (−1) multiplizieren".to_string(),
                "| ·(−1)".to_string(),
                format!("Beide Seiten mit (−1) multiplizieren, damit {target} allein steht (und nicht −{target})."),
            )
        } else {
            (
                format!("Durch {} teilen", paren(coefficient)),
                format!("| :{}", paren(coefficient)),
                format!(
                    "Beide Seiten durch {} teilen, damit {target} allein steht: {} : {} = {result}.",
                    paren(coefficient),
                    paren(value),
                    paren(coefficient)
                ),
            )
        };
        let mut step = Step::new(title, explanation, self.line()).with_operation(operation);
        if flips {
            step.hint = format!("Wir rechnen mit einer negativen Zahl: Das Vorzeichen kehrt sich um – aus {value} wird {result}.");
        }
        steps.push(step);
        self.right = plain(&self.right);
    }

    /// `var² = value`.
    fn square_root(&self, steps: &mut Vec<Step>, value: Rational) -> Vec<Root> {
        let v = self.var;
        if value.is_negative() {
            steps.push(Step::new(
                "Keine Lösung",
                format!(
                    "{v}² = {value} geht nicht: Ein Quadrat ist nie negativ, denn Plus mal Plus und Minus mal Minus \
                     ergeben beide Plus. Darum gibt es keine Lösung."
                ),
                text_line(format!("{v}² = {value}")),
            ));
            return Vec::new();
        }
        if value.is_zero() {
            steps.push(Step::new("Wurzel ziehen", format!("Nur 0 · 0 ergibt 0, also ist {v} = 0."), text_line(format!("{v} = 0"))));
            return vec![Root::exact(Rational::ZERO)];
        }
        let radicand = if value.is_integer() { value.to_string() } else { format!("({value})") };
        let (positive, negative) = match value.sqrt() {
            Some(root) => (Root::exact(root), Root::exact(-root)),
            None => {
                let approx = value.to_f64().sqrt();
                (Root::irrational(format!("√{radicand}"), approx), Root::irrational(format!("−√{radicand}"), -approx))
            }
        };
        let line = self.two_roots(&positive, &negative);
        let explanation = format!(
            "Die Wurzel ziehen: {v} = ±√{radicand}. Es gibt zwei Lösungen, denn auch die negative Zahl ergibt im Quadrat {value}."
        );
        let hint = format!(
            "Nicht vergessen: ({})² = {value} – Minus mal Minus ergibt Plus. Darum ist auch {} eine Lösung.",
            negative.text, negative.text
        );
        steps.push(Step::new("Wurzel ziehen", explanation, line).with_hint(hint));
        vec![positive, negative]
    }

    fn two_roots(&self, first: &Root, second: &Root) -> Line {
        let mut line = Line::new();
        push_text(&mut line, format!("{}₁ = ", self.var), None, false);
        first.push(&mut line, true);
        push_text(&mut line, format!("      {}₂ = ", self.var), None, false);
        second.push(&mut line, true);
        line
    }

    /// `a·x² + b·x + c = 0` with `a ≠ 0` and `b ≠ 0`.
    fn quadratic(&mut self, steps: &mut Vec<Step>) -> Vec<Root> {
        if !self.right.is_empty() {
            let movers = self.right.clone();
            self.move_terms(steps, &movers, true, "Terme");
        }
        let (mut a, mut b, mut c) = (self.coefficient(2), self.coefficient(1), self.coefficient(0));
        if a.is_negative() {
            self.left = self.left.iter().map(|t| t.neg().marked(Mark { color: None, sign: true })).collect();
            let explanation = format!(
                "Die Zahl vor {}² ist negativ. Mit (−1) multipliziert wird sie positiv – das macht das Weiterrechnen einfacher.",
                self.var
            );
            steps.push(
                Step::new("Mit (−1) multiplizieren", explanation, self.line())
                    .with_operation("| ·(−1)")
                    .with_hint("Mal (−1): Jedes Vorzeichen kehrt sich um, aus + wird − und aus − wird +."),
            );
            self.left = plain(&self.left);
            (a, b, c) = (-a, -b, -c);
        }
        if c.is_zero() { self.factor_out(steps, a, b) } else { self.formula(steps, a, b, c) }
    }

    /// `a·x² + b·x = 0` → `x(a·x + b) = 0`.
    fn factor_out(&self, steps: &mut Vec<Step>, a: Rational, b: Rational) -> Vec<Root> {
        let v = self.var;
        let linear = Sum::from_terms(vec![Mono::new(a, Vars::single(v, 1)), Mono::constant(b)]);
        let product = Sum(vec![Summand::new(vec![
            Factor::Mono(self.power(1).marked(Mark::color(0))),
            Factor::Group(Group { sum: linear.clone(), power: 1 }),
        ])]);
        steps.push(Step::new(
            format!("{v} ausklammern"),
            format!("{v} kommt in beiden Termen vor, also klammern wir {v} aus."),
            equation_line(&product, &Sum::default()),
        ));

        let second = -b / a;
        let linear = linear.text();
        let explanation = format!(
            "Ein Produkt ist 0, wenn einer der Faktoren 0 ist. Also ist entweder {v} = 0 oder {linear} = 0. \
             Aus {linear} = 0 folgt {v} = {second}."
        );
        let constant = Mono::constant(b);
        let hint = format!(
            "Beim Auflösen von {linear} = 0 wechselt {} auf die andere Seite und wird zu {}.",
            constant.signed_text(),
            constant.neg().signed_text()
        );
        let roots = vec![Root::exact(Rational::ZERO), Root::exact(second)];
        steps.push(Step::new("Nullprodukt", explanation, self.two_roots(&roots[0], &roots[1])).with_hint(hint));
        roots
    }

    /// The quadratic formula ("Mitternachtsformel").
    fn formula(&self, steps: &mut Vec<Step>, a: Rational, b: Rational, c: Rational) -> Vec<Root> {
        let v = self.var;
        let mut line = text_line(format!("{v}₁,₂ = "));
        line.push(Token { glyph: Glyph::Fraction("−b ± √(b² − 4ac)".into(), "2a".into()), color: None, sign: false });
        steps.push(Step::new(
            "Lösungsformel",
            format!(
                "Die Gleichung hat die Form a{v}² + b{v} + c = 0 mit a = {a}, b = {b} und c = {c}. \
                 Jetzt hilft die Lösungsformel (Mitternachtsformel):"
            ),
            line,
        ));

        let four_ac = Rational::int(4) * a * c;
        let d = b * b - four_ac;
        let mut hints = Vec::new();
        if b.is_negative() {
            hints.push(format!("{}² = {}: Minus mal Minus ergibt Plus – darum gehört {b} in Klammern!", paren(b), b * b));
        }
        if four_ac.is_negative() {
            hints.push(format!("4·{}·{} = {four_ac} ist negativ, also wird − {} zu + {}.", paren(a), paren(c), paren(four_ac), -four_ac));
        }
        let discriminant =
            format!("D = b² − 4ac = {}² − 4·{}·{} = {} − {} = {d}", paren(b), paren(a), paren(c), b * b, paren(four_ac));
        steps.push(
            Step::new(
                "Diskriminante",
                "Zuerst den Teil unter der Wurzel ausrechnen, die Diskriminante D. Sie entscheidet: \
                 D > 0 → zwei Lösungen, D = 0 → eine Lösung, D < 0 → keine Lösung.",
                text_line(discriminant),
            )
            .with_hint(hints.join("\n")),
        );

        if d.is_negative() {
            steps.push(Step::new(
                "Keine Lösung",
                format!("D = {d} ist negativ. Aus einer negativen Zahl kann man keine Wurzel ziehen – die Gleichung hat keine Lösung."),
                text_line(format!("D = {d} < 0")),
            ));
            return Vec::new();
        }
        let (minus_b, two_a) = (-b, Rational::int(2) * a);
        let minus_b_hint = format!("−b heisst: das Vorzeichen von b umkehren. b = {b}, also −b = {minus_b}.");
        let fraction = |numerator: String| Token { glyph: Glyph::Fraction(numerator, two_a.to_string()), color: None, sign: false };

        if d.is_zero() {
            let x = minus_b / two_a;
            let mut line = text_line(format!("{v} = "));
            line.push(fraction(minus_b.to_string()));
            push_text(&mut line, " = ", None, false);
            push_number(&mut line, x, None, false);
            steps.push(Step::new("Lösung berechnen", "D = 0: Die Wurzel ist 0, es gibt nur eine Lösung.", line).with_hint(minus_b_hint));
            return vec![Root::exact(x)];
        }

        let (roots, root_text, explanation) = match d.sqrt() {
            Some(s) => (
                [Root::exact((minus_b + s) / two_a), Root::exact((minus_b - s) / two_a)],
                s.to_string(),
                format!("√D = √{d} = {s}. Einmal mit +, einmal mit − rechnen:"),
            ),
            None => {
                let s = d.to_f64().sqrt();
                let at = |sign: f64| (minus_b.to_f64() + sign * s) / two_a.to_f64();
                let text = |op: char| format!("({minus_b} {op} √{d}) / {two_a}");
                (
                    [Root::irrational(text('+'), at(1.0)), Root::irrational(text('−'), at(-1.0))],
                    format!("√{d}"),
                    format!("√{d} geht nicht auf, darum geben wir die Lösungen gerundet an. Einmal mit +, einmal mit − rechnen:"),
                )
            }
        };
        let mut line = Line::new();
        for (i, (root, op)) in roots.iter().zip(['+', '−']).enumerate() {
            let gap = if i == 0 { "" } else { "      " };
            push_text(&mut line, format!("{gap}{v}{} = ", if i == 0 { '₁' } else { '₂' }), None, false);
            line.push(fraction(format!("{minus_b} {op} {root_text}")));
            push_text(&mut line, if root.exact.is_some() { " = " } else { " ≈ " }, None, false);
            match root.exact {
                Some(value) => push_number(&mut line, value, None, false),
                None => push_text(&mut line, approx(root.approx), None, false),
            }
        }
        steps.push(Step::new("Lösungen berechnen", explanation, line).with_hint(minus_b_hint));
        roots.into()
    }

    fn solution_set(&self, roots: &[Root]) -> Step {
        let v = self.var;
        let mut line = text_line("L = {");
        for (i, root) in roots.iter().enumerate() {
            if i > 0 {
                push_text(&mut line, "; ", None, false);
            }
            root.push(&mut line, false);
        }
        push_text(&mut line, if roots.is_empty() { " }" } else { "}" }, None, false);
        let explanation = match roots {
            [] => "Die Gleichung hat keine Lösung, die Lösungsmenge ist leer.".to_string(),
            [root] => format!("Die Gleichung hat genau eine Lösung: {v} = {}.", root.text),
            [first, second, ..] => format!("Die Gleichung hat zwei Lösungen: {v}₁ = {} und {v}₂ = {}.", first.text, second.text),
        };
        Step::new("Lösungsmenge", explanation, line)
    }

    /// Substitutes `value` into the simplified original equation.
    fn probe(&self, original: &(Vec<Mono>, Vec<Mono>), value: Rational) -> Step {
        let v = self.var;
        let evaluate = |terms: &[Mono]| {
            terms.iter().fold(Rational::ZERO, |acc, t| acc + t.coeff * value.pow(t.vars.degree()))
        };
        let (left, right) = (evaluate(&original.0), evaluate(&original.1));
        let line = text_line(format!(
            "{} = {}   →   {left} = {right} ✓",
            substituted(&original.0, value),
            substituted(&original.1, value)
        ));
        let explanation = format!(
            "{v} = {value} in die vereinfachte Gleichung einsetzen: Links ergibt sich {left}, rechts {right}. \
             Beide Seiten sind gleich, die Lösung stimmt."
        );
        Step::new(format!("Probe für {v} = {value}"), explanation, line)
    }
}

/// The terms with the variable replaced by `value`, e.g. `3·(−2)² − 4`.
fn substituted(terms: &[Mono], value: Rational) -> String {
    if terms.is_empty() {
        return "0".into();
    }
    let base = if value.is_negative() || !value.is_integer() { format!("({value})") } else { value.to_string() };
    terms
        .iter()
        .enumerate()
        .map(|(i, term)| {
            let sign = match (i, term.coeff.is_negative()) {
                (0, false) => "",
                (0, true) => "−",
                (_, false) => " + ",
                (_, true) => " − ",
            };
            let coeff = term.coeff.abs();
            let body = match term.vars.degree() {
                0 => coeff.to_string(),
                degree => {
                    let power = if degree == 1 { base.clone() } else { format!("{base}{}", super::tree::superscript(degree)) };
                    if coeff.is_one() { power } else { format!("{coeff}·{power}") }
                }
            };
            format!("{sign}{body}")
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::super::tree::line_text;
    use super::*;

    fn solution(input: &str) -> String {
        let steps = steps(input).unwrap();
        let set = steps.iter().find(|s| s.title == "Lösungsmenge").expect("a solution set");
        line_text(&set.line)
    }

    fn titles(input: &str) -> Vec<String> {
        steps(input).unwrap().into_iter().map(|s| s.title).collect()
    }

    #[test]
    fn solves_linear_equations() {
        assert_eq!(solution("3x + 4 = 19"), "L = {5}");
        assert_eq!(solution("5x - 7 = 2x + 8"), "L = {5}");
        assert_eq!(solution("3(x - 2) + 4 = 2x - 5"), "L = {−3}");
        assert_eq!(solution("x/2 + 1 = x/3 + 2"), "L = {6}");
        assert_eq!(solution("-4x + 3 = 2(x - 6)"), "L = {5/2}");
        assert_eq!(solution("12 = 4y"), "L = {3}");
        assert_eq!(solution("x² + 2x = x² + 4"), "L = {2}");
    }

    #[test]
    fn shows_moving_terms_with_sign_change() {
        let steps = steps("5x - 7 = 2x + 8").unwrap();
        let moved = steps.iter().find(|s| s.title == "x-Terme nach links bringen").unwrap();
        assert_eq!(moved.operation, "| −2x");
        assert_eq!(line_text(&moved.line), "5x − 7 − 2x = 2x + 8 − 2x");
        assert!(moved.hint.contains("aus +2x rechts wird −2x links"));
        assert!(moved.line.iter().any(|t| t.sign));
        let divided = steps.iter().find(|s| s.title == "Durch 3 teilen").unwrap();
        assert_eq!(line_text(&divided.line), "x = 5");
        assert!(titles("5x - 7 = 2x + 8").contains(&"Probe für x = 5".to_string()));
    }

    #[test]
    fn dividing_by_a_negative_number_flips_the_sign() {
        let steps = steps("-3x = 12").unwrap();
        let divided = steps.iter().find(|s| s.title == "Durch (−3) teilen").unwrap();
        assert_eq!(line_text(&divided.line), "x = −4");
        assert!(divided.hint.contains("aus 12 wird −4"));
    }

    #[test]
    fn handles_contradictions_and_identities() {
        assert!(titles("2x + 1 = 2x + 3").contains(&"Widerspruch".to_string()));
        assert!(titles("2(x + 1) = 2x + 2").contains(&"Immer wahr".to_string()));
        assert!(titles("3 = 3").contains(&"Immer wahr".to_string()));
    }

    #[test]
    fn solves_quadratic_equations() {
        assert_eq!(solution("x² - 5x + 6 = 0"), "L = {3; 2}");
        assert_eq!(solution("2x² = 18"), "L = {3; −3}");
        assert_eq!(solution("x² + 3x = 0"), "L = {0; −3}");
        assert_eq!(solution("x^2 + 1 = 0"), "L = { }");
        assert_eq!(solution("x² - 4x + 4 = 0"), "L = {2}");
        assert_eq!(solution("x² = 2"), "L = {√2; −√2}");
        assert_eq!(solution("-x² + 5x = 6"), "L = {3; 2}");
        assert_eq!(solution("x² + x = 1"), "L = {(−1 + √5) / 2; (−1 − √5) / 2}");
    }

    #[test]
    fn rejects_unsupported_equations() {
        assert_eq!(steps("x + y = 2").unwrap_err(), AlgebraError::MultipleVariables("x, y".into()));
        assert_eq!(steps("x^3 = 8").unwrap_err(), AlgebraError::DegreeTooHigh);
        assert_eq!(steps("x + 2").unwrap_err(), AlgebraError::MissingEquals);
    }
}
