# Fractions Visualizer

A simple destop app to visualize fractions using using [Slint](https://slint.rs) for the user interface.

## Usage

Build the app using cargo run and then simply enter a fraction and press the "Bruch anzeigen" button.

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
