
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

fn main() {
    SimpleLogger::new()
        .with_module_level("tungstenite", LevelFilter::Warn)
        .init()
        .unwrap();

    let mut server = Server::new("127.0.0.1:8080");
    let mut index = 0;

    loop {
        for connection in &mut server.connections() {
            let mut gui = connection.gui::<MyId>();
            let root = gui.root();
            let (left, right) = root.vertical_panels();

            // Left
            let mut stack = left.stacklayout();
            stack.header("The left side".to_owned());
            if stack.button().finish() {
                println!("Button Unnamed");
            }
            if stack.button().text("Button 2".to_owned()).finish() {
                println!("Button 2");
            }
            for i in 0..40 {
                if (i + index) % 13 == 1 {
                    stack.button().text(format!("{}", i)).finish();
                } else {
                    let (l, r) = stack.layout().vertical_panels();
                    l.stacklayout().label(i ^ index);
                    r.stacklayout()
                        .checkbox()
                        .text(format!("{} Active", i + 1))
                        .finish();
                }
            }
            let area51 = stack.layout();
            let (lower_left, lower_right) = area51.vertical_panels();
            let mut lower_left_stack = lower_left.stacklayout();
            lower_left_stack.header("L");
            lower_left_stack.button().finish();
            let mut lower_right_stack = lower_right.stacklayout();
            lower_right_stack.header("R");
            lower_right_stack.button().finish();

            // Right
            let mut stack = right.stacklayout();
            stack.header("The right side".to_owned());
            stack.button().finish();
            stack.button().text("Button 4".to_owned()).finish();
            for i in 0..10 {
                if stack.button().handle(&i).text(format!("Button {}", i)).finish() {
                    println!("Button {}", i);
                }
            }
            if index > 20 {
                stack
                    .button()
                    .text("Late button")
                    .finish();
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
