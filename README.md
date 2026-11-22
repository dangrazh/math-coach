# Mathe Coach

A simple desktop app to practise fractions and algebra, using [Slint](https://slint.rs) for the user interface.

## Usage

Build the app using cargo run. The start menu leads to the two trainers:

- **Brüche** – a sub-menu with two parts:
  - **Brüche darstellen** – enter a fraction and press the "Bruch anzeigen" button to see it visualized.
  - **Kürzen & Erweitern** – step-by-step cancelling and expanding of fractions (see below).
  - **Rechnen mit Brüchen** – step-by-step multiplying, dividing, adding and subtracting of fractions, also in longer
    calculations with brackets (see below).
- **Terme & Gleichungen** – step-by-step solutions for terms and equations (see below).

The app needs a larger main-thread stack than the Windows default (the whole window with all screens is built on
it); `build.rs` sets 8 MB for MSVC builds.

## Visualizations

Pick how the fraction is drawn with the buttons below the input:

- **Kreis** – 2D pie chart
- **Brett (3D)** – a board of square tiles (cuboids 1/3 as high as they are wide); enter its size as rows × columns, or leave both empty for the most square board fitting the denominator
- **Dreieck** – a right isosceles triangle tiled by smaller right isosceles triangles; this works for k² or 2·k² pieces (1, 2, 4, 8, 9, 16, 18, 25, 32, …)
- **Würfel (3D)** – a block of small cubes (the most cube-like `x × y × z` arrangement), filled bottom-up; empty cubes are see-through

### Larger groups

Every shape takes a number of elements (sectors, tiles, triangles, cubes) that must be a multiple of the denominator.
The elements are then split into *denominator* equal groups and the first *numerator* groups are highlighted –
e.g. 5/8 on an 8 × 8 board shows 8 areas of 8 tiles, 5 of them colored. Each group gets an orange outline, and neighbouring groups get different shades; both can be switched off with the "Gruppen umranden" and "Gruppen einfärben" checkboxes.
With **‹ ›** next to "Gruppen" you can pick any equivalent fraction whose denominator divides the number of
elements, from the largest possible groups (the reduced fraction, e.g. 4/8 on 64 tiles → 1/2 = 2 groups of 32) down to
single elements (32/64). Use **‹ ›** next to "Aufteilung" to switch between the available groupings (blocks of different shapes, larger triangles,
row by row, …).

The 3D views can be rotated a full 360° around the X and Z axes with the sliders or by dragging the drawing with the mouse.
Mixed numbers are drawn as one completely filled shape per whole next to the fractional part (denominator ≤ 100, at most 4 wholes).

## Cancelling and expanding fractions

Five tabs with the same step-by-step layout as the equation trainer (see below). Fractions are entered with one
fraction bar, `Zähler / Nenner`; sums belong in brackets, e.g. `(4x + 8)/(6x + 12)`. In every step, factors that
occur above and below are underlined in the same colour, and cancelled factors are struck through like on paper.

- **Kürzen** – numbers only: split numerator and denominator into prime factors, mark the common ones (their
  product is the ggT), strike them; improper results also get the mixed number
- **Erweitern** – numbers only: `3/4 mit 5` (by a factor) or `3/4 auf 20` (to a new denominator, which first
  finds the factor 20 : 4 = 5)
- **Variablen kürzen** – sums are factored first (common factor, binomial formulas), then numbers are split into
  ggT · rest and powers are written out (`a² = a·a`) before cancelling; a minus sign moves in front of the fraction
- **Variablen erweitern** – `2a/3b mit 4a`, `x/(x + 1) mit (x − 1)` or `1/(x − 2) auf x² − 4` (the new
  denominator must contain the old one as a factor)
- **Potenzen kürzen** – equal bases are marked and cancelled with the exponent rule, shown as `x⁷⁻⁴`; this
  also works for brackets like `(x + 1)⁵/(x + 1)²`

Numbers in fractions must be integers.

## Calculating with fractions

Four tabs – **Multiplizieren**, **Dividieren**, **Addieren**, **Subtrahieren** – with the same step-by-step layout.
Enter two numbers with one operator: `2/3 · 3/4`, `3/4 : 1/2`, `1/4 + 2/3`, `3/4 - 1/6`. Whole numbers (`3`),
mixed numbers (`1 1/2`) and negative numbers (`-5/6`, `1/2 - (-1/3)`) are accepted; division uses `:` because `/` is
the fraction bar. Every step says what is done and why:

- whole and mixed numbers are first written as fractions (`1 1/2 = (1·2 + 1)/2 = 3/2`)
- **multiplying**: decide the sign, write everything on one fraction bar (numerator times numerator, denominator
  times denominator), cancel before multiplying – also crosswise, with coloured and struck-through factors – then multiply
- **dividing**: multiply by the reciprocal (highlighted), then continue as for multiplying
- **adding / subtracting**: resolve double signs, reduce the fractions first, find the common denominator as the
  kgV (listing the multiples), expand both fractions (factors in colour), add or subtract the numerators, reduce the result
- improper results also get the mixed number

A fifth tab, **Gemischte Rechnungen**, takes longer calculations with any of the four operators and brackets, e.g.
`1/2 + 1/3 · 3/4` or `2/3 · (1/4 + 1/2) - 1/6` (up to 8 operations). It first states the order rules that apply –
brackets first (innermost first), multiplication and division before addition and subtraction, otherwise left to
right – and then works through one sub-calculation at a time:

- **Als Nächstes** marks the next sub-calculation in the whole calculation and says which rule makes it next
- the sub-calculation is solved with the same steps as in the single-operation tabs
- **Teilergebnis einsetzen** puts its result back into the calculation (marked), dropping brackets that are no longer
  needed and applying a minus in front of brackets

## Terms and equations

Four tabs share the same layout: type a term (or an equation) into the input field and press **Los geht's** /
**Nächster Schritt** to reveal the solution one step at a time (**Alle Schritte** shows everything at once,
**Beispiel** fills in an example). Every step has a German explanation and shows the whole expression:

- terms that belong together (like terms, the products of an expansion, the common factor) are underlined in the same colour,
- signs that a step changes or decides are highlighted in yellow, with a note explaining the rule
  (minus before a bracket, minus times minus, moving a term to the other side, dividing by a negative number, …),
- equation steps show the operation applied to both sides on the right, e.g. `| −2x`.

The tabs:

- **Vereinfachen** – multiply monomials (`3a·4b`), resolve `±(…)`, mark, reorder and combine like terms
- **Ausmultiplizieren** – expand `k(a + b)`, `(a + b)(c + d)` and powers like `(a + b)²`, innermost brackets first
- **Ausklammern** – factor out the greatest common factor (numbers and variables; a negative one if the first term is negative), with a check by expanding again
- **Gleichungen lösen** – linear and quadratic equations in one variable: simplify both sides, clear fractions,
  move variable terms left and numbers right, divide; quadratics via `x² = k`, factoring out `x`, or the
  quadratic formula; the solution set is checked by substitution

Input accepts `*`, `·`, `/`, `:`, `^`, `²`, `³`, implicit multiplication (`2x`, `3(x + 1)`, `(x + 1)(x − 1)`)
and decimal numbers with `.` or `,`. Calculations are exact (fractions), so `x/2 + x/3` stays `5/6x`.
