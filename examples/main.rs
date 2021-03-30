use iwgui::*;

use log::LevelFilter;
use simple_logger::SimpleLogger;
use std::{thread, time::Duration};

struct Duck {
    name: String,
    in_the_water: bool,
}

struct PaperPlane {
    paper_size: usize,
    name: String,
}

struct Model {
    ducks_at_the_pont: Vec<Duck>,
    paper_planes: Vec<PaperPlane>,
}

impl Model {
    fn example() -> Self {
        Self {
            ducks_at_the_pont: vec![
                Duck {
                    name: String::from("Robin"),
                    in_the_water: false,
                },
                Duck {
                    name: String::from("Jenny"),
                    in_the_water: true,
                },
                Duck {
                    name: String::from("Melissa"),
                    in_the_water: false,
                },
            ],
            paper_planes: Vec::new(),
        }
    }
}

fn main() {
    SimpleLogger::new()
        .with_module_level("tungstenite", LevelFilter::Warn)
        .init()
        .unwrap();

    let mut server = Server::new("127.0.0.1:8080");
    let mut model = Model::example();
    loop {
        for connection in &mut server.connections() {
            let mut gui = connection.gui();
            let root = gui.root();
            let (left, right) = root.vertical_panels();

            // Build the left side of the GUI
            ducks(left, &mut model.ducks_at_the_pont);

            // Build the right side of the GUI
            paper_planes(right, &mut model.paper_planes);

            connection.show_gui(gui);
        }
        thread::sleep(Duration::from_millis(50));
    }
}

fn ducks(left: Indeterminate, ducks_at_the_pont: &mut Vec<Duck>) {
    let mut stack = left.stacklayout();
    stack.header("Ducks at the Pont".to_owned());
    if stack.button().text("Wave arms").finish() {
        println!("Waving arms like a lunatic");
    }
    for duck in ducks_at_the_pont {
        let (l, r) = stack.layout().vertical_panels();
        l.stacklayout()
            .label(format!("{} = {}", &duck.name, duck.in_the_water))
            .handle(&PtrHandle::new(duck))
            .finish();
        let handle = PtrHandle::new(duck);
        r.stacklayout()
            .checkbox(&mut duck.in_the_water)
            .handle(&handle)
            .text("In the water")
            .finish();
    }
    let lower = stack.layout();
    let (lower_left, lower_right) = lower.vertical_panels();
    let mut lower_left_stack = lower_left.stacklayout();
    lower_left_stack.header("Left side");
    if lower_left_stack.button().text("Throw bread").finish() {
        println!("Throwing bread from the left side");
    }
    let mut lower_right_stack = lower_right.stacklayout();
    lower_right_stack.header("Right side");
    if lower_right_stack.button().text("Throw bread").finish() {
        println!("Throwing bread from the right side");
    }
}

fn paper_planes(right: Indeterminate, paper_planes: &mut Vec<PaperPlane>) {
    let mut stack = right.stacklayout();
    if stack.button().text("New Paper Plane").finish() {
        paper_planes.push(PaperPlane {
            paper_size: 1,
            name: "unknown".to_owned(),
        });
    }
    for (index, paper_plane) in paper_planes.iter_mut().enumerate() {
        let (l, r) = stack.layout().vertical_panels();
        let (l, m) = l.vertical_panels();
        let handle = PtrHandle::new(paper_plane);
        l.stacklayout()
            .label(format!("Plane {}", index))
            .handle(&index)
            .finish();
        m.stacklayout()
            .text_box(&mut paper_plane.name)
            .handle(&index)
            .finish();
        r.stacklayout()
            .number(&mut paper_plane.paper_size)
            .handle(&handle)
            .text("Paper Size")
            .finish()
            .unwrap();
    }
}
