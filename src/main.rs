use std::{cell::RefCell, collections::BTreeMap, io::{Read, Write}, marker::PhantomData, net::{TcpListener, TcpStream, ToSocketAddrs}, slice::IterMut, sync::{Arc, Mutex, MutexGuard}, thread, time::Duration};

use log::{debug, error, info, trace};
use simple_logger::SimpleLogger;
use tungstenite::{Message, WebSocket};
use uuid::Uuid;
use serde::{Serialize, Deserialize};


#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Serialize, Deserialize)]
enum MyId {
    Any,
    Button1,
    Button2,
}

impl Default for MyId {
    fn default() -> Self {
        MyId::Any
    }
}
impl Id for MyId {}

trait Id: Default + Sync + Send + Eq + Ord + Copy {}

fn main() {
    SimpleLogger::new().init().unwrap();

    let mut server = Server::<MyId>::new("127.0.0.1:8080");
    let mut index = 0;
    loop {
        for connection in &mut server.connections() {
            let mut gui = Gui::new();
            let root = gui.root();
            let (left, right) = root.vertical_panels();

            // Left
            let mut stack = left.stacklayout();
            stack.header("The left side".to_owned());
            stack
                .button()
                .finish();
            stack
                .button()
                .text("Button 2".to_owned())
                .finish();
            for i in 0..40 {
                stack.label(i ^ index);
            }

            // Right
            let mut stack = right.stacklayout();
            stack.header("The right side".to_owned());
            stack
                .button()
                .finish();
            stack
                .button()
                .text("Button 4".to_owned())
                .finish();
            for i in 0..10 {
                stack
                    .button()
                    .text(format!("Button {}", i))
                    .finish();
            }
            connection.show_gui(&gui);
            index += 1;
        }
        thread::sleep(Duration::from_millis(50));
    }
}

struct GuiState<I: Id> {
    root: Option<I>,
    elements: BTreeMap<I, Element<I>>,
}

impl<I: Id> GuiState<I> {
    fn fetch_id(&mut self) -> I {
        let result = I::default();
        result
    }
}

struct Gui<I: Id> {
    state: RefCell<GuiState<I>>,
}

impl<'gui, I: Id> Gui<I> {
    fn new() -> Self {
        Self {
            state: RefCell::new(GuiState {
                root: None,
                elements: BTreeMap::new(),
            })
        }
    }

    // TODO: Ensure that this works when called multiple times
    fn root(&'gui mut self) -> Indeterminate<'gui, I> {
        let mut state = self.state.borrow_mut();
        let id = state.fetch_id();
        state.elements.insert(id, Element::Indeterminate);
        state.root = Some(id);
        Indeterminate::new(&self.state, id)
    }

    fn to_html(&self) -> Option<String> {
        let state = self.state.borrow_mut();
        state.root.map(|root| {
            state.elements
                .get(&root)
                .expect("must be inserted")
                .to_html(&*state)
        })
    }
}

// ----------------------------------------------------------------------------
// Indeterminate
// ----------------------------------------------------------------------------

struct Indeterminate<'gui, I: Id> {
    state: &'gui RefCell<GuiState<I>>,
    target_id: I,
}

impl<'gui, I: Id> Indeterminate<'gui, I> {
    fn new(state: &'gui RefCell<GuiState<I>>, target_id: I) -> Self {
        Self { state, target_id }
    }

    fn stacklayout(self) -> StackLayout<'gui, I> {
        let mut state = self.state.borrow_mut();
        let element = Element::StackLayout {
            children: Vec::new(),
        };
        *state
            .elements
            .get_mut(&self.target_id)
            .expect("must be inserted") = element;
        StackLayout {
            state: self.state,
            id: self.target_id,
        }
    }

    fn vertical_panels(self) -> (Indeterminate<'gui, I>, Indeterminate<'gui, I>) {
        let mut state = self.state.borrow_mut();
        let left = state.fetch_id();
        let right = state.fetch_id();
        state.elements.insert(left, Element::Indeterminate);
        state.elements.insert(right, Element::Indeterminate);
        *state
            .elements
            .get_mut(&self.target_id)
            .expect("must be inserted") = Element::Columns { left, right };
        (
            Indeterminate::new(self.state, left),
            Indeterminate::new(self.state, right),
        )
    }
}

// ----------------------------------------------------------------------------
// StackLayout
// ----------------------------------------------------------------------------

struct StackLayout<'gui, I: Id> {
    state: &'gui RefCell<GuiState<I>>,
    id: I,
}

impl<I: Id> PushElement<I> for StackLayout<'_, I> {
    fn push_element(&mut self, id: I, element: Element<I>) {
        let mut state = self.state.borrow_mut();
        state.elements.insert(id, element);
        let stacklayout = state
            .elements
            .get_mut(&self.id)
            .expect("must be inserted upon generation of StackLayout");
        match stacklayout {
            Element::StackLayout { children } => children.push(id),
            _ => panic!("wrong element inserted"),
        }
    }

    fn gui(&mut self) -> &RefCell<GuiState<I>> {
        self.state
    }
}

// ----------------------------------------------------------------------------
// Columns
// ----------------------------------------------------------------------------

struct Columns<'gui, I: Id> {
    gui: &'gui mut Gui<I>,
    id: I,
}

// ----------------------------------------------------------------------------
// ButtonBuilder
// ----------------------------------------------------------------------------

struct ButtonBuilder<'parent, I: Id, P: PushElement<I>> {
    parent: &'parent mut P,
    id: I,
    text: Option<String>,
    phantom: PhantomData<I>,
}

impl<'parent, I: Id, P: PushElement<I>> ButtonBuilder<'parent, I, P> {
    pub fn text(mut self, text: String) -> Self {
        self.text = Some(text);
        self
    }

    pub fn finish(self) {
        self.parent.push_element(self.id, Element::new_button());
    }
}

// ----------------------------------------------------------------------------
// traits
// ----------------------------------------------------------------------------

trait PushElement<I: Id>: Sized {
    fn push_element(&mut self, id: I, element: Element<I>);
    fn gui(&mut self) -> &RefCell<GuiState<I>>;

    fn header(&mut self, text: String) {
        self.push_element(I::default(), Element::Header(text))
    }

    fn label<T: ToString>(&mut self,value: T) {
        self.push_element(I::default(), Element::Label(value.to_string()))
    }

    #[must_use = "The finish method has to be called on the ButtonBuilder to create a button."]
    fn button(&mut self) -> ButtonBuilder<I, Self> {
        ButtonBuilder {
            parent: self,
            id: I::default(),
            text: None,
            phantom: PhantomData,
        }
    }

    fn layout<'gui>(&'gui mut self) -> Indeterminate<'gui, I> {
        let id = self.gui().borrow_mut().fetch_id();
        self.push_element(id, Element::Indeterminate);
        Indeterminate::new(self.gui(), id)
    }
}

// ----------------------------------------------------------------------------
// Element
// ----------------------------------------------------------------------------

#[derive(Debug, PartialEq, Eq)]
enum Element<I: Id> {
    Indeterminate,
    Header(String),
    Label(String),
    Button {
        id: I,
        text: Option<String>,
    },
    StackLayout {
        children: Vec<I>, // OPTIMIZE: Get rid of heap allocation
    },
    Columns {
        left: I,
        right: I,
    },
}

impl<I: Id> Element<I> {
    fn new_button() -> Element<I> {
        Element::Button { 
            id: I::default(),
            text: None,
        }
    }

    fn to_html(&self, state: &GuiState<I>) -> String {
        match self {
            Element::Indeterminate => "".to_owned(),
            Element::Header(text) => format!("<h1>{}</h1>", text),
            Element::Label(value) => format!("<div>{}</div>", value),
            Element::Button { id, text } => {
                let text = text.clone().unwrap_or_else(|| "Button".to_owned());
                format!("<button>{}</button>", text)
            }
            Element::StackLayout { children } => {
                let children = children
                    .iter()
                    .map(|id| state.elements.get(id).expect("must be inserted").to_html(state))
                    .collect::<Vec<_>>()
                    .join("\n");
                format!("<div>{}</div>", children)
            }
            Element::Columns { left, right } => {
                let left = state
                    .elements
                    .get(left)
                    .expect("must be inserted")
                    .to_html(state);
                let right = state
                    .elements
                    .get(right)
                    .expect("must be inserted")
                    .to_html(state);
                format!("<div class=\"row\"><div class=\"column\">{}</div><div class=\"column\">{}</div></div>", left, right)
            }
        }
    }
}

// ----------------------------------------------------------------------------
//
// ----------------------------------------------------------------------------

struct Connection<I: Id> {
    uuid: Uuid,
    websocket: WebSocket<TcpStream>,
    last_gui: Option<Gui<I>>,
}

impl<I: Id> Connection<I> {
    pub fn show_gui(&mut self, gui: &Gui<I>) {
        if let Some(last_gui) = &self.last_gui {
            let last_state = last_gui.state.borrow();
            let new_state = gui.state.borrow();
            let mut added = Vec::new();
            let mut removed = Vec::new();
            let mut updated = Vec::new();
            for (old_id, old_element) in &last_state.elements {
                match new_state.elements.get(old_id) {
                    None => removed.push(old_id),
                    Some(new) if new != old_element => updated.push(old_element),
                    Some(_) => (),
                }
            }
            for (new_id, new_element) in &new_state.elements {
                match last_state.elements.get(new_id) {
                    None => added.push(new_element),
                    Some(_) => (), // case handed above
                }
            }
        } else {
            if let Some(html) = gui.to_html() {
                self.websocket.write_message(Message::Text(html)).unwrap(); // TODO: Error handling
            }
        }
    }
}

struct Connections<'a, I: Id> {
    r: MutexGuard<'a, Vec<Connection<I>>>,
}

impl<'a, 'b: 'a, I: Id> IntoIterator for &'a mut Connections<'b, I> {
    type IntoIter = IterMut<'a, Connection<I>>;
    type Item = &'a mut Connection<I>;
    fn into_iter(self) -> IterMut<'a, Connection<I>> {
        self.r.iter_mut()
    }
}

struct Server<I: Id> {
    connections: Arc<Mutex<Vec<Connection<I>>>>,
}

impl<I: 'static + Id> Server<I> {
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
                                                last_gui: None,
                                            };
                                            let mut connections = connections3.lock().unwrap(); // Error Handling
                                            connections.push(connection);
                                            let connections_array = connections
                                                .iter()
                                                .map(|c| c.uuid.to_string())
                                                .collect::<Vec<String>>()
                                                .join(", ");
                                            debug!(
                                                "Connections: {}",
                                                format!("[{}]", connections_array)
                                            );
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

    pub fn connections<'a>(&mut self) -> Connections<I> {
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
            Err(err) => panic!(
                "Could not read from stream of connection {}: {}",
                address, err
            ), // TODO: Error handling
        }
    });
}

#[cfg(test)]
mod tests {

    #[test]
    fn main_loop() {}
}
