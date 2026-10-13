#![windows_subsystem = "windows"]

mod fraction;
mod visual;

use std::rc::Rc;

use slint::{SharedString, VecModel};

use fraction::MixedFraction;
use visual::{Elements, GroupDisplay, Scene, Selection, View};

slint::include_modules!();

/// Parses an optional positive count; an empty field means "automatic".
fn parse_count(text: &str) -> Result<Option<u32>, String> {
    match text.trim() {
        "" => Ok(None),
        t => t.parse().ok().filter(|&n| n > 0).map(Some).ok_or_else(|| format!("«{t}» ist keine gültige Anzahl.")),
    }
}

/// Reads the element settings of the current mode from the UI.
fn elements(ui: &AppWindow) -> Result<Elements, String> {
    let count = |text: SharedString| parse_count(&text).map(|c| c.map_or(Elements::Auto, Elements::Count));
    match ui.get_mode() {
        VisualMode::Pie => count(ui.get_pie_count()),
        VisualMode::Triangle => count(ui.get_triangle_count()),
        VisualMode::Cube => count(ui.get_cube_count()),
        VisualMode::Board => match (parse_count(&ui.get_board_rows())?, parse_count(&ui.get_board_cols())?) {
            (Some(rows), Some(cols)) => Ok(Elements::Grid { rows, cols }),
            (None, None) => Ok(Elements::Auto),
            _ => Err("Bitte Zeilen und Spalten angeben (oder beide leer lassen).".into()),
        },
    }
}

fn scene(ui: &AppWindow, view: View) -> Result<Scene, String> {
    let Fraction { integer, numerator, denominator } = ui.get_fraction();
    let fraction = MixedFraction::from_input(integer, numerator, denominator).map_err(|e| e.to_string())?;
    let display = GroupDisplay { shading: ui.get_show_group_shading(), outline: ui.get_show_group_outline() };
    // A negative group choice means "use the entered denominator".
    let choice = ui.get_group_choice();
    let selection = Selection { group_choice: (choice >= 0).then_some(choice), variant: ui.get_variant_index() };
    visual::build(ui.get_mode(), fraction, elements(ui)?, selection, display, view).map_err(|e| e.to_string())
}

fn main() -> Result<(), slint::PlatformError> {
    let ui = AppWindow::new()?;
    let weak_ui = ui.as_weak();

    ui.on_render(move || {
        let Some(ui) = weak_ui.upgrade() else { return };

        let view = View::new(f64::from(ui.get_rot_x()), f64::from(ui.get_rot_z()));
        ui.set_rot_x(view.rot_x as f32);
        ui.set_rot_z(view.rot_z as f32);

        let (scene, message) = match scene(&ui, view) {
            Ok(scene) => (scene, String::new()),
            Err(message) => (Scene::default(), message),
        };

        let Scene { shapes, width, height, group_choice, group_labels, variant, variant_labels, summary, grouped } = scene;
        ui.set_shapes(Rc::new(VecModel::from(shapes)).into());
        ui.set_view_width(width.max(visual::CELL) as f32);
        ui.set_view_height(height.max(visual::CELL) as f32);
        // Keep "entered denominator" (-1) until the user picks a group count.
        if ui.get_group_choice() >= 0 {
            ui.set_group_choice(group_choice as i32);
        }
        ui.set_group_count(group_labels.len() as i32);
        ui.set_group_label(group_labels.get(group_choice).cloned().unwrap_or_default().into());
        ui.set_group_position(group_choice as i32);
        ui.set_variant_index(variant as i32);
        ui.set_variant_count(variant_labels.len() as i32);
        ui.set_variant_label(variant_labels.get(variant).cloned().unwrap_or_default().into());
        ui.set_summary(summary.into());
        ui.set_grouped(grouped);
        ui.set_message(message.into());
    });

    ui.run()
}
