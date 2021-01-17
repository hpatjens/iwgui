mod connection;
mod gui;

use connection::*;
use gui::*;
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
            let events = connection.events::<MyId>();
            for event in events {
                info!("{:?}", event);
            }

            let mut gui = Gui::empty();
            let root = gui.root();
            let (left, right) = root.vertical_panels();

            // Left
            let mut stack = left.stacklayout();
            stack.header("The left side".to_owned());
            stack.button().handle(MyId::Button1).finish();
            stack
                .button()
                .handle(MyId::Button2)
                .text("Button 2".to_owned())
                .finish();
            for i in 0..40 {
                if (i + index) % 13 == 1 {
                    stack.button().text(format!("{}", i)).finish();
                } else {
                    stack.label(i ^ index);
                }
            }

            // Right
            let mut stack = right.stacklayout();
            stack.header("The right side".to_owned());
            stack.button().finish();
            stack.button().text("Button 4".to_owned()).finish();
            for i in 0..10 {
                stack
                    .button()
                    .handle(MyId::RightButton(i))
                    .text(format!("Button {}", i))
                    .finish();
            }
            if index > 20 {
                stack
                    .button()
                    .handle(MyId::LateButton)
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
