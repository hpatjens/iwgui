
use iwgui::*;

use log::{info, LevelFilter};
use serde::{Deserialize, Serialize};
use simple_logger::SimpleLogger;
use std::{thread, time::Duration};

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Serialize, Deserialize)]
enum MyId {
    Any,
    Button1,
    Button2,
    RightButton(usize),
    LateButton,
}

impl Default for MyId {
    fn default() -> Self {
        MyId::Any
    }
}
impl Id for MyId {
    fn to_string(&self) -> String {
        match self {
            MyId::Any => String::from("Any"),
            MyId::Button1 => String::from("Button1"),
            MyId::Button2 => String::from("Button2"),
            MyId::RightButton(i) => format!("RightButton.{}", i),
            MyId::LateButton => String::from("LateButton"),
        }
    }

    // TODO: Maybe use Result with error message
    fn from_str(s: &str) -> Option<Self> {
        match s {
            "Any" => Some(MyId::Any),
            "Button1" => Some(MyId::Button1),
            "Button2" => Some(MyId::Button2),
            "LateButton" => Some(MyId::LateButton),
            s => {
                const PREFIX: &'static str = "RightButton.";
                if s.starts_with(PREFIX) {
                    if let Ok(i) = s[PREFIX.len()..].parse::<usize>() {
                        return Some(MyId::RightButton(i));
                    }
                }
                None
            }
        }
    }
}

struct Duck {
    name: String,
    in_the_water: bool,
}

struct PaperPlane {
    paper_size: usize, // TODO: Replace this with an enum

}

struct Model {
    ducks_at_the_pont: Vec<Duck>,
    paper_planes: Vec<PaperPlane>,
}

fn main() {
    SimpleLogger::new()
        .with_module_level("tungstenite", LevelFilter::Warn)
        .init()
        .unwrap();

    let mut server = Server::new("127.0.0.1:8080");
    let mut index = 0;

    let mut model = Model {
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
            }
        ],
        paper_planes: Vec::new(),
    };

    loop {
        for connection in &mut server.connections() {
            let mut gui = connection.gui::<MyId>();
            let root = gui.root();
            let (left, right) = root.vertical_panels();

            // Left
            let mut stack = left.stacklayout();
            stack.header("Ducks at the Pont".to_owned());
            stack.checkbox().text("Are we really doing this?").finish();
            if stack.button().text("Wave arms").finish() {
                println!("Waving arms like a lunatic");
            }
            for duck in &model.ducks_at_the_pont {
                let (l, r) = stack.layout().vertical_panels();
                l.stacklayout().label(&duck.name);
                r.stacklayout()
                    .checkbox()
                    .handle_from_ptr(&*duck)
                    .text("In the water")
                    .finish();
            }
            let area51 = stack.layout();
            let (lower_left, lower_right) = area51.vertical_panels();
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

            // Right
            let mut stack = right.stacklayout();
            if stack.button().text("New Paper Plane").finish() {
                model.paper_planes.push(PaperPlane { paper_size: 1 });
            }
            for (index, paper_plane) in model.paper_planes.iter().enumerate() {
                stack.label(format!("Plane {}", index));
            }

            connection.show_gui(gui);
            index += 1;
        }
        thread::sleep(Duration::from_millis(50));
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn main_loop() {}
}
