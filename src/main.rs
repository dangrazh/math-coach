// #![allow(unused)]
#![windows_subsystem = "windows"]

// use core::num;
use std::f64::consts::PI;
use slint::VecModel;
use std::rc::Rc;

slint::include_modules!();


struct Coordinates {
    x: f64,
    y: f64,
}
#[derive(Debug)]
struct Visualization {
    is_integer: bool,
    no_of_pies: i32,
    no_of_sectors: i32,
    sectors: Vec<Sector>,
}

impl Visualization {
    pub fn new() -> Self {
        Visualization {
            is_integer: false,
            no_of_pies: 0,
            no_of_sectors: 0,
            sectors: Vec::new(),
        }
    }
}

fn main() -> Result<(), slint::PlatformError> {
    let ui = AppWindow::new()?;
    let weak_ui = ui.as_weak();

    ui.on_display_result({
        // let sector_model = sector_model.clone();
        // sector_model.clear();
        move |fraction| {
            let ui_w = weak_ui.unwrap();                    
            let visualization = calc_visualization(fraction);
            // print_sectors(&sectors);
            // for sector in sectors {
            //     sector_model.push(sector)
            // }
            
            let sector_model = Rc::new(VecModel::<Sector>::from(visualization.sectors));
            ui_w.set_sector_model(sector_model.clone().into());


            let mut pies: Vec<Pie> = Vec::new();
            for _i in 0..visualization.no_of_pies {
                pies.push(Pie { no_of_sectors: visualization.no_of_sectors, is_integer: visualization.is_integer });
            }
            // println!("pies: {:?}", pies);

            let pie_model = Rc::new(VecModel::<Pie>::from(pies));
            ui_w.set_pie_model(pie_model.clone().into());
        }

    });

    ui.run()
}

// fn print_sectors(sectors: &Vec<Sector>) {
//     for sector in sectors {
//         println!("d_string: {}, active: {}", sector.path_d_string, sector.active);
//     }
// }

fn calc_visualization(fraction: Fraction) -> Visualization {

    let mut int = fraction.integer;
    let mut numerator = fraction.numerator;
    let denominator = fraction.denominator;
    let mut d: String;
    let mut sectors: Vec<Sector> = Vec::new();
    let mut out = Visualization::new();

    // println!("int: {}, numerator: {}, denominator: {}", int, numerator, denominator);

    match (int, numerator, denominator) {
       (0, 0, 0) => {
        
        // process the no numbers provided path
        // println!("Processing no numbers provided path");
        d = get_d_string(100f64, 0f64, 360f64); 
        // println!("d string for empty input: {}", d); 
        sectors.push(Sector {path_d_string: d.into(), active: true});
        out.no_of_pies = 1;
        out.no_of_sectors = 1;
        out.sectors = sectors;

       }, 
       (_, 0, 0) => {
        
        // process integer only - draw (a) circle(s)
        // println!("Processing integer only path");
        for _i in 0..int.abs() {
            d = get_d_string(100f64, 0f64, 180f64); 
            sectors.push(Sector {path_d_string: d.into(), active: true});
            d = get_d_string(100f64, 180f64, 360f64); 
            sectors.push(Sector {path_d_string: d.into(), active: true});

        }
        out.is_integer = true;
        out.no_of_pies = int.abs();
        out.no_of_sectors = 2;
        out.sectors = sectors;

       }, 
       (_, _, _) => {
        
        // process a full fraction
        // println!("Processing full fraction path");
        let sector_degrees: f64 = 360f64 / denominator.abs() as f64;

        // preprocess fractions with numerator > denominator and move whole numbers 
        // into the 'int' variable and the modulo back into the numerator
        if numerator > denominator {
            int = int + (numerator / denominator);
            numerator = numerator % denominator;
        }
        
        if int.abs() > 0 {
            // integer processing part - calculate <denominator> sectors of size <denominator> and set all of them to active
            for _int_cnt in 0..int.abs() {
                for i in 0..denominator.abs() {
                    let start_angle: f64 = i as f64 * sector_degrees;
                    let end_angle: f64 = start_angle + sector_degrees;
                    d = get_d_string(100f64, start_angle, end_angle); 
                    // println!("d string for 1/{} sector: {}", denominator, d); 
                    sectors.push(Sector {path_d_string: d.into(), active: true});                    
                }
            }
    
        }

        // fration processing part - calculate <denominator> sectors of size <denominator> and set them to active if index <= numerator
        for i in 0..denominator.abs() {
            let start_angle: f64 = i as f64 * sector_degrees;
            let end_angle: f64 = start_angle + sector_degrees;
            d = get_d_string(100f64, start_angle, end_angle); 
            // println!("d string for 1/{} sector: {}", denominator, d); 

            if i < numerator {
                sectors.push(Sector {path_d_string: d.into(), active: true});    
            } else {
                sectors.push(Sector {path_d_string: d.into(), active: false});    
            }
    
        }

        // construct the final Visualization struct
        out.no_of_pies = int.abs()+1;
        out.no_of_sectors = denominator;
        out.sectors = sectors;
       },
    }

    // return the vector of sectors
    out


}

fn get_d_string(radius: f64, start_angle: f64, end_angle: f64) -> String {
    let is_circle: bool = end_angle - start_angle == 360f64;

    let start = polar_to_cartesian(radius,start_angle);
    let end = polar_to_cartesian(radius, end_angle);
    
    let mut large_arc_flag = 1;
    if end_angle - start_angle <= 180f64 {
        large_arc_flag = 0;    
    }

    let mut d_string: Vec<String> = Vec::new();
    d_string.push("M".to_string());
    d_string.push(start.x.to_string());
    d_string.push(start.y.to_string());
    
    d_string.push("A".to_string());
    d_string.push(radius.to_string());
    d_string.push(radius.to_string());
    d_string.push("0".to_string());
    d_string.push(large_arc_flag.to_string());
    d_string.push("1".to_string());
    d_string.push(end.x.to_string());
    d_string.push(end.y.to_string());

    if is_circle {
        d_string.push("Z".to_string());
    } else {
        d_string.push("L".to_string());
        d_string.push(radius.to_string());
        d_string.push(radius.to_string());
        d_string.push("L".to_string());
        d_string.push(start.x.to_string());
        d_string.push(start.y.to_string());
        d_string.push("Z".to_string());            
    }


    d_string.join(" ")
    
}

fn polar_to_cartesian(radius: f64, angle_in_degrees: f64) -> Coordinates {
    let radians = (angle_in_degrees - 90f64) * PI / 180f64;
    let x = f64::round(radius + (radius * f64::cos(radians)));
    let y = f64::round(radius + (radius * f64::sin(radians)));
    
    Coordinates{x, y}
}