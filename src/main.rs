use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream, ToSocketAddrs},
    slice::IterMut,
    sync::{Arc, Mutex, MutexGuard},
    thread,
    time::Duration,
};

use log::{debug, error, info, trace};
use simple_logger::SimpleLogger;
use id_arena::{Arena, Id};
use tungstenite::{Message, WebSocket};
use uuid::Uuid;

fn main() {
    SimpleLogger::new().init().unwrap();

    let mut server = Server::new("127.0.0.1:8080");
    loop {
        for connection in &mut server.connections() {
            let mut gui = Gui { arena: Arena::new() };
            let mut stack = gui.stacklayout();
            stack.button().finish();
            stack.button().text("Button 2".to_owned()).finish();
            connection.show_gui(stack);
        }
        thread::sleep(Duration::from_millis(50));
    }
}

struct Gui {
    arena: Arena<Element>,
}

impl<'gui> Gui {
    fn stacklayout(&'gui mut self) -> StackLayout<'gui> {
        let id = self.arena.alloc(Element::StackLayout {
            children: Vec::new(),
        });
        StackLayout { 
            gui: self,
            id,
        }
    }
}

// ----------------------------------------------------------------------------
// StackLayout
// ----------------------------------------------------------------------------

struct StackLayout<'gui> {
    gui: &'gui mut Gui,
    id: Id<Element>,
}

impl<'gui> StackLayout<'gui> {
    #[must_use = "The finish method has to be called on the ButtonBuilder to create a button."]
    fn button(&mut self) -> ButtonBuilder<StackLayout<'gui>> {
        ButtonBuilder {
            parent: self,
            text: None,
        }
    }
}

impl ToHtml for StackLayout<'_> {
    fn to_html(&self) -> String {
        self.gui.arena
            .get(self.id)
            .expect("must be inserted upon generation")
            .to_html(&self.gui)
    }
}

impl PushElement for StackLayout<'_> {
    fn push_element(&mut self, element: Element) {
        let id = self.gui.arena.alloc(element);
        let stacklayout = self.gui.arena
            .get_mut(self.id)
            .expect("must be inserted upon generation of StackLayout");
        match stacklayout {
            Element::StackLayout { children } => children.push(id),
            _ => panic!("wrong element inserted"),
        }
    }
}

// ----------------------------------------------------------------------------
// ButtonBuilder
// ----------------------------------------------------------------------------

struct ButtonBuilder<'parent, P: PushElement> {
    parent: &'parent mut P,
    text: Option<String>,
}

impl<'parent, P: PushElement> ButtonBuilder<'parent, P> {
    pub fn text(mut self, text: String) -> Self {
        self.text = Some(text);
        self
    }

    pub fn finish(self) {
        self.parent.push_element(Element::new_button());
    }
}

// ----------------------------------------------------------------------------
// traits
// ----------------------------------------------------------------------------

trait PushElement {
    fn push_element(&mut self, element: Element);
}

trait ToHtml {
    fn to_html(&self) -> String;
}

// ----------------------------------------------------------------------------
// Element
// ----------------------------------------------------------------------------

#[derive(Debug)]
enum Element {
    Button {
        text: Option<String>,
    },
    StackLayout {
        children: Vec<Id<Element>>, // OPTIMIZE: Get rid of heap allocation
    }, 
}

impl Element {
    fn new_button() -> Element {
        Element::Button { text: None }
    }

    fn to_html(&self, gui: &Gui) -> String {
        match self {
            Element::Button { text } => {
                let text = text
                    .clone()
                    .unwrap_or_else(|| "Button".to_owned());
                format!("<button>{}</button>", text)
            }
            Element::StackLayout { children } => {
                let children = children
                    .iter()
                    .map(|id| gui.arena.get(*id).expect("must be inserted").to_html(gui))
                    .collect::<Vec<_>>().join("\n");
                format!("<div>\n{}\n<div>", children)
            }
        }
    }
}

// ----------------------------------------------------------------------------
// 
// ----------------------------------------------------------------------------

struct Connection {
    uuid: Uuid,
    websocket: WebSocket<TcpStream>,
}

impl Connection {
    pub fn show_gui<H: ToHtml>(&mut self, gui: H) {
        let html = gui.to_html();
        self.websocket.write_message(Message::Text(html)).unwrap(); // TODO: Error handling
    }
}

struct Connections<'a> {
    r: MutexGuard<'a, Vec<Connection>>,
}

impl<'a, 'b: 'a> IntoIterator for &'a mut Connections<'b> {
    type IntoIter = IterMut<'a, Connection>;
    type Item = &'a mut Connection;
    fn into_iter(self) -> IterMut<'a, Connection> {
        self.r.iter_mut()
    }
}

struct Server {
    connections: Arc<Mutex<Vec<Connection>>>,
}

impl Server {
    // TODO: IP
    pub fn new<A: ToSocketAddrs + Send + 'static>(address: A) -> Self {
        let connections = Arc::new(Mutex::new(Vec::new()));
        let connections2 = connections.clone();
        thread::spawn(move || {
            let listener = TcpListener::bind(address).unwrap(); // TODO: Error handling
            for stream in listener.incoming() {
                match stream {
                    Ok(stream) => handle_incoming_connection(stream),
                    Err(err) => {
                        panic!("Could not retrieve incoming stream of connection: {}", err);
                        // TODO: Error handling
                    }
                }
            }
        });
        thread::spawn(move || {
            let server = TcpListener::bind("127.0.0.1:9001").unwrap();
            for stream in server.incoming() {
                info!("Incoming websocket connection");
                match stream {
                    Ok(stream) => {
                        let connections3 = connections2.clone();
                        thread::spawn(move || {
                            info!("Websocket connection thread");
                            match tungstenite::server::accept(stream) {
                                Ok(websocket) => {
                                    info!("Websocket connection accepted");
                                    let mut websocket = websocket;
                                    let msg = websocket.read_message().unwrap();
                                    match msg {
                                        tungstenite::Message::Text(text) => {
                                            trace!("Received message:\n{}", text);
                                            let connection = Connection {
                                                websocket,
                                                uuid: Uuid::new_v4(),
                                            };
                                            let mut connections = connections3.lock().unwrap(); // Error Handling
                                            connections.push(connection);
                                            let connections_array = connections
                                                .iter()
                                                .map(|c| c.uuid.to_string())
                                                .collect::<Vec<String>>()
                                                .join(", ");
                                            debug!("Connections: {}", format!("[{}]", connections_array));
                                        }
                                        _ => info!("Expected greeting message"),
                                    }
                            }
                                Err(err) => {
                                    error!("{}", err);
                                }
                            }
                        });
                    }
                    Err(err) => {
                        error!("{}", err);
                    }
                }
            }
        });
        Self { connections }
    }

    pub fn connections<'a>(&mut self) -> Connections {
        let connections = self.connections.lock().unwrap(); // TODO: Error handling
        Connections { r: connections }
    }
}

fn handle_incoming_connection(mut stream: TcpStream) {
    let address = stream
        .peer_addr()
        .map(|a| a.to_string())
        .unwrap_or_else(|_| "unknown".to_owned());
    info!("Incoming connection from {}", address);
    thread::spawn(move || {
        info!("Created connection thread");
        let mut buffer = [0; 1024]; // TODO: How to handle this?
        match stream.read(&mut buffer) {
            Ok(0) => info!("Zero bytes were read from the stream."),
            Ok(_bytes_read) => {
                info!("Read bytes on connection {}", address);
                let contents = include_str!("../web/index.html");
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{}",
                    contents.len(),
                    contents
                );

                stream.write(response.as_bytes()).unwrap();
                stream.flush().unwrap();
                info!("index.html sent");
            }
            Err(err) => panic!("Could not read from stream of connection {}: {}", address, err), // TODO: Error handling
        }
    });
}

#[cfg(test)]
mod tests {

    #[test]
    fn main_loop() {}
}
