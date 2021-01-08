use std::{cell::RefCell, collections::BTreeMap, io::{Read, Write}, marker::PhantomData, net::{TcpListener, TcpStream, ToSocketAddrs}, slice::IterMut, sync::{Arc, Mutex, MutexGuard}, thread, time::Duration};

use log::{debug, error, info, trace};
use simple_logger::SimpleLogger;
use tungstenite::{Message, WebSocket};
use uuid::Uuid;
use serde::{Serialize, Deserialize};


#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Serialize, Deserialize)]
enum MyId {
    Any,
    Button1,
    Button2,
    RightButton(usize),
}

impl Default for MyId {
    fn default() -> Self {
        MyId::Any
    }
}
impl<'id> Id<'id> for MyId {}

trait Id<'id>: Default + Sync + Send + Eq + Ord + Copy + Serialize + Deserialize<'id> {}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Serialize)]
enum GuiId<I> 
where 
    for<'id> I: Id<'id>
{
    Auto(usize),
    User(I),
}

fn main() {
    SimpleLogger::new().init().unwrap();

    let mut server = Server::<MyId>::new("127.0.0.1:8080");
    let mut index = 0;
    loop {
        for connection in &mut server.connections() {
            let events = connection.events();
            dbg!(&events);
            if !events.is_empty() {
                panic!();
            }

            let mut gui = Gui::new();
            let root = gui.root();
            let (left, right) = root.vertical_panels();

            // Left
            let mut stack = left.stacklayout();
            stack.header("The left side".to_owned());
            stack
                .button()
                .handle(MyId::Button1)
                .finish();
            stack
                .button()
                .handle(MyId::Button2)
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
                    .handle(MyId::RightButton(i))
                    .text(format!("Button {}", i))
                    .finish();
            }
            connection.show_gui(&gui);
            index += 1;
        }
        thread::sleep(Duration::from_millis(50));
    }
}

struct GuiState<I>
where
    for<'id> I: Id<'id>
{
    next_id: usize,
    root: Option<GuiId<I>>,
    elements: BTreeMap<GuiId<I>, Element<I>>,
}

impl<I> GuiState<I>
where
    for<'id> I: Id<'id>
{
    fn fetch_id(&mut self) -> GuiId<I> {
        let result = GuiId::Auto(self.next_id);
        self.next_id += 1;
        result
    }
}

struct Gui<I>
where
    for<'id> I: Id<'id>
{
    state: RefCell<GuiState<I>>,
}

impl<'gui, I> Gui<I>
where 
    for<'id> I: Id<'id>
{
    fn new() -> Self {
        Self {
            state: RefCell::new(GuiState {
                next_id: 0,
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
                .to_html(&root, &*state)
        })
    }
}

// ----------------------------------------------------------------------------
// Indeterminate
// ----------------------------------------------------------------------------

struct Indeterminate<'gui, I>
where
    for<'id> I: Id<'id>
{
    state: &'gui RefCell<GuiState<I>>,
    target_id: GuiId<I>,
}

impl<'gui, I> Indeterminate<'gui, I>
where 
    for<'id> I: Id<'id>
{
    fn new(state: &'gui RefCell<GuiState<I>>, target_id: GuiId<I>) -> Self {
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

struct StackLayout<'gui, I>
where
    for<'id> I: Id<'id>
{
    state: &'gui RefCell<GuiState<I>>,
    id: GuiId<I>,
}

impl<I> PushElement<I> for StackLayout<'_, I> 
where
    for<'id> I: Id<'id>
{
    fn push_element(&mut self, id: GuiId<I>, element: Element<I>) {
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

struct Columns<'gui, I>
where
    for<'id> I: Id<'id>
{
    gui: &'gui mut Gui<I>,
    id: GuiId<I>,
}

// ----------------------------------------------------------------------------
// ButtonBuilder
// ----------------------------------------------------------------------------

struct ButtonBuilder<'parent, I, P: PushElement<I>>
where 
    for<'id> I: Id<'id>
{
    parent: &'parent mut P,
    id: Option<I>,
    text: Option<String>,
    phantom: PhantomData<I>,
}

impl<'parent, I, P: PushElement<I>> ButtonBuilder<'parent, I, P>
where 
    for<'id> I: Id<'id>
{
    pub fn text(mut self, text: String) -> Self {
        self.text = Some(text);
        self
    }

    pub fn handle(mut self, id: I) -> Self {
        self.id = Some(id);
        self
    }

    pub fn finish(self) {
        let id = self.id
            .map(|id| GuiId::User(id))
            .unwrap_or_else(|| self.parent.gui().borrow_mut().fetch_id());
        self.parent.push_element(id, Element::new_button(self.text));
    }
}

// ----------------------------------------------------------------------------
// traits
// ----------------------------------------------------------------------------

trait PushElement<I>: Sized
where
    for<'id> I: Id<'id>
{
    fn push_element(&mut self, id: GuiId<I>, element: Element<I>);
    fn gui(&mut self) -> &RefCell<GuiState<I>>;

    fn header(&mut self, text: String) {
        let id = self.gui().borrow_mut().fetch_id();
        self.push_element(id, Element::Header(text))
    }

    fn label<T: ToString>(&mut self,value: T) {
        let id = self.gui().borrow_mut().fetch_id();
        self.push_element(id, Element::Label(value.to_string()))
    }

    #[must_use = "The finish method has to be called on the ButtonBuilder to create a button."]
    fn button(&mut self) -> ButtonBuilder<I, Self> {
        ButtonBuilder {
            parent: self,
            id: None,
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
enum Element<I>
where 
    for<'id> I: Id<'id>
{
    Indeterminate,
    Header(String),
    Label(String),
    Button {
        text: Option<String>,
    },
    StackLayout {
        children: Vec<GuiId<I>>, // OPTIMIZE: Get rid of heap allocation
    },
    Columns {
        left: GuiId<I>,
        right: GuiId<I>,
    },
}

impl<I> Element<I>
where 
    for<'id> I: Id<'id>
{
    fn new_button(text: Option<String>) -> Element<I> {
        Element::Button { 
            text,
        }
    }

    fn to_html(&self, id: &GuiId<I>, state: &GuiState<I>) -> String {
        match self {
            Element::Indeterminate => "".to_owned(),
            Element::Header(text) => format!("<h1>{}</h1>", text),
            Element::Label(value) => format!("<div>{}</div>", value),
            Element::Button { text } => {
                let text = text.clone().unwrap_or_else(|| "Button".to_owned());
                if let GuiId::User(user_id) = id {
                    // TODO: Use the event
                    let event = serde_json::to_string(&user_id).unwrap().replace("\"", "'");


                    format!("<button onclick=\"send_event()\">{}</button>", text)
                } else {
                    format!("<button>{}</button>", text)
                }
            }
            Element::StackLayout { children } => {
                let children = children
                    .iter()
                    .map(|id| state.elements.get(id).expect("must be inserted").to_html(id, state))
                    .collect::<Vec<_>>()
                    .join("\n");
                format!("<div>{}</div>", children)
            }
            Element::Columns { left, right } => {
                let left = state.elements
                    .get(left)
                    .expect("must be inserted")
                    .to_html(left, state);
                let right = state.elements
                    .get(right)
                    .expect("must be inserted")
                    .to_html(right, state);
                format!("<div class=\"row\"><div class=\"column\">{}</div><div class=\"column\">{}</div></div>", left, right)
            }
        }
    }
}

// ----------------------------------------------------------------------------
//
// ----------------------------------------------------------------------------

struct Connection<I>
where
    for<'id> I: Id<'id>
{
    uuid: Uuid,
    websocket: WebSocket<TcpStream>,
    last_gui: Option<Gui<I>>,
}

impl<I> Connection<I> 
where
    for<'id> I: Id<'id>
{
    pub fn events(&mut self) -> Vec<I> {
        let mut events: Vec<I> = Vec::new();
        // loop {
        //     match self.websocket.read_message() {
        //         Ok(message) => {
        //             match message {
        //                 Message::Text(text) => {
        //                     events.push(serde_json::from_str(&text).expect("malformed event"));
        //                 }
        //                 _ => {},
        //             }
        //             break;
        //         }
        //         Err(tungstenite::Error::Io(err)) if err.kind() == std::io::ErrorKind::WouldBlock => {
        //             break;
        //         }
        //         Err(err) => {
        //             panic!("Error while receiving an event: {}", err)
        //         }
        //     }
        // }
        events
    }

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

struct Connections<'a, I> 
where
    for<'id> I: Id<'id>
{
    r: MutexGuard<'a, Vec<Connection<I>>>,
}

impl<'a, 'b: 'a, I> IntoIterator for &'a mut Connections<'b, I> 
where
    for<'id> I: Id<'id>
{
    type IntoIter = IterMut<'a, Connection<I>>;
    type Item = &'a mut Connection<I>;
    fn into_iter(self) -> IterMut<'a, Connection<I>> {
        self.r.iter_mut()
    }
}

const WEBSOCKET_ADDRESS1: &'static str = "127.0.0.1:9001";
const WEBSOCKET_ADDRESS2: &'static str = "127.0.0.1:9002";

struct Server<I> 
where
    for<'id> I: Id<'id>
{
    connections: Arc<Mutex<Vec<Connection<I>>>>,
}

impl<I> Server<I> 
where
    for<'id> I: 'static + Id<'id>
{
    // TODO: IP
    pub fn new<A: ToSocketAddrs + Send + 'static>(address: A) -> Self {
        let connections = Arc::new(Mutex::new(Vec::new()));
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
        spawn_incoming_thread(WEBSOCKET_ADDRESS1, connections.clone());
        Self { connections }
    }

    pub fn connections<'a>(&mut self) -> Connections<I> {
        let connections = self.connections.lock().unwrap(); // TODO: Error handling
        Connections { r: connections }
    }

}


fn spawn_incoming_thread<I>(address: &'static str, connections: Arc<Mutex<Vec<Connection<I>>>>)
where 
    for<'id> I: 'static + Id<'id>
{
    thread::spawn(move || {
        let server = TcpListener::bind(address).unwrap();
        for stream in server.incoming() {
            info!("Incoming websocket connection");
            match stream {
                Ok(stream) => {
                    handle_incoming_websocket_connection(stream, connections.clone());
                }
                Err(err) => {
                    error!("{}", err);
                }
            }
        }
    });
}

fn handle_incoming_websocket_connection<I>(stream: TcpStream, connections: Arc<Mutex<Vec<Connection<I>>>>)
where 
    for<'id> I: 'static + Id<'id>
{
    thread::spawn(move || {
        info!("Started websocket connection thread");
        match tungstenite::server::accept(stream) {
            Ok(websocket) => {
                info!("Websocket connection accepted");
                let connection = Connection {
                    websocket,
                    uuid: Uuid::new_v4(),
                    last_gui: None,
                };
                let mut connections = connections.lock().unwrap(); // Error Handling
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
            Err(err) => {
                error!("{}", err);
            }
        }
    });
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
