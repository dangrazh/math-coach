# Mathe Coach

A simple desktop app to practise fractions and algebra, using [Slint](https://slint.rs) for the user interface.

## Usage

Build the app using cargo run. The start menu leads to the two trainers:

- **Brüche** – enter a fraction and press the "Bruch anzeigen" button to see it visualized.
- **Terme & Gleichungen** – step-by-step solutions for terms and equations (see below).

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
