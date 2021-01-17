use std::{cell::RefCell, collections::BTreeMap, fmt, io::{Read, Write}, marker::PhantomData, mem, net::{TcpListener, TcpStream, ToSocketAddrs}, slice::IterMut, sync::{Arc, Mutex, MutexGuard}, thread, time::Duration};

use log::{LevelFilter, debug, error, info, trace, warn};
use simple_logger::SimpleLogger;
use tungstenite::{Message, WebSocket, error::Error};
use uuid::Uuid;
use serde::{Serialize, Deserialize};

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

pub trait Id: fmt::Debug + Default + Sync + Send + Eq + Ord + Copy {
    fn to_string(&self) -> String;
    // TODO: Maybe use Result with error message
    fn from_str(s: &str) -> Option<Self>;
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Serialize)]
#[serde(transparent)]
pub struct GuiId(String);

impl GuiId {
    fn new_auto(i: usize) -> Self {
        GuiId(format!("Auto.{}", i))
    }

    fn new_user<I: Id>(id: I) -> Self {
        GuiId(format!("User.{}", id.to_string()))
    }

    // TODO: Maybe use Result with error message
    fn from_str<I: Id>(s: &str) -> Option<Self> {
        const PREFIX_AUTO: &'static str = "Auto.";
        const PREFIX_USER: &'static str = "User.";
        if s.starts_with(PREFIX_AUTO) {
            if let Ok(i) = s[PREFIX_AUTO.len()..].parse::<usize>() {
                return Some(GuiId::new_auto(i));
            }
        }
        if s.starts_with(PREFIX_USER) {
            if let Some(id) = I::from_str(&s[PREFIX_USER.len()..]) {
                return Some(GuiId::new_user(id));
            }
        }
        None
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
                if (i + index) % 13 == 1 {
                    stack.button().text(format!("{}", i)).finish();
                } else {
                    stack.label(i ^ index);
                }
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

struct GuiState {
    next_id: usize,
    root: Option<GuiId>,
    elements: BTreeMap<GuiId, Element>,
}

impl GuiState {
    fn fetch_id(&mut self) -> GuiId {
        let result = GuiId::new_auto(self.next_id);
        self.next_id += 1;
        result
    }
}

#[derive(Debug)]
struct GuiDiff {
    only_lhs: Vec<GuiId>,
    only_rhs: Vec<GuiId>,
    unequal: Vec<GuiId>,
}

struct Gui {
    state: RefCell<GuiState>,
}

impl<'gui> Gui {
    fn new() -> Self {
        Self {
            state: RefCell::new(GuiState {
                next_id: 0,
                root: None,
                elements: BTreeMap::new(),
            })
        }
    }

    fn diff(lhs: &Gui, rhs: &Gui) -> GuiDiff {
        let lhs_state = lhs.state.borrow();
        let rhs_state = rhs.state.borrow();
        let mut only_lhs = Vec::new();
        let mut only_rhs = Vec::new();
        let mut unequal = Vec::new();
        for (lhs_id, lhs_element) in &lhs_state.elements {
            match rhs_state.elements.get(lhs_id) {
                None => only_lhs.push(lhs_id.clone()),
                Some(rhs_element) if rhs_element != lhs_element => unequal.push(lhs_id.clone()),
                Some(_) => {},
            }
        }
        for rhs_id in rhs_state.elements.keys() {
            match lhs_state.elements.get(rhs_id) {
                None => only_rhs.push(rhs_id.clone()),
                Some(_) => {},
            }
        }
        GuiDiff { only_lhs, only_rhs, unequal }
    }

    // TODO: Ensure that this works when called multiple times
    fn root(&'gui mut self) -> Indeterminate<'gui> {
        let mut state = self.state.borrow_mut();
        let id = state.fetch_id();
        state.elements.insert(id.clone(), Element::Indeterminate);
        state.root = Some(id.clone());
        Indeterminate::new(&self.state, id)
    }
}

// ----------------------------------------------------------------------------
// Indeterminate
// ----------------------------------------------------------------------------

struct Indeterminate<'gui> {
    state: &'gui RefCell<GuiState>,
    target_id: GuiId,
}

impl<'gui> Indeterminate<'gui> {
    fn new(state: &'gui RefCell<GuiState>, target_id: GuiId) -> Self {
        Self { state, target_id }
    }

    fn stacklayout(self) -> StackLayout<'gui> {
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

    fn vertical_panels(self) -> (Indeterminate<'gui>, Indeterminate<'gui>) {
        let mut state = self.state.borrow_mut();
        let left = state.fetch_id();
        let right = state.fetch_id();
        state.elements.insert(left.clone(), Element::Indeterminate);
        state.elements.insert(right.clone(), Element::Indeterminate);
        *state
            .elements
            .get_mut(&self.target_id)
            .expect("must be inserted") = Element::Columns { 
                left: left.clone(), 
                right: right.clone(),
            };
        (
            Indeterminate::new(self.state, left),
            Indeterminate::new(self.state, right),
        )
    }
}

// ----------------------------------------------------------------------------
// StackLayout
// ----------------------------------------------------------------------------

struct StackLayout<'gui> {
    state: &'gui RefCell<GuiState>,
    id: GuiId,
}

impl PushElement for StackLayout<'_> {
    fn push_element(&mut self, id: GuiId, element: Element) {
        let mut state = self.state.borrow_mut();
        state.elements.insert(id.clone(), element);
        let stacklayout = state
            .elements
            .get_mut(&self.id)
            .expect("must be inserted upon generation of StackLayout");
        match stacklayout {
            Element::StackLayout { children } => children.push(id),
            _ => panic!("wrong element inserted"),
        }
    }

    fn gui(&mut self) -> &RefCell<GuiState> {
        self.state
    }
}

// ----------------------------------------------------------------------------
// ButtonBuilder
// ----------------------------------------------------------------------------

struct ButtonBuilder<'parent, P: PushElement> {
    parent: &'parent mut P,
    id: Option<GuiId>,
    text: Option<String>,
}

impl<'parent, P: PushElement> ButtonBuilder<'parent, P> {
    pub fn text<S: Into<String>>(mut self, text: S) -> Self {
        self.text = Some(text.into());
        self
    }

    pub fn handle<I: Id>(mut self, id: I) -> Self {
        self.id = Some(GuiId::new_user(id));
        self
    }

    pub fn finish(self) {
        let id = self.id.clone().unwrap_or_else(|| self.parent.gui().borrow_mut().fetch_id());
        self.parent.push_element(id, Element::new_button(self.text));
    }
}

// ----------------------------------------------------------------------------
// traits
// ----------------------------------------------------------------------------

trait PushElement: Sized {
    fn push_element(&mut self, id: GuiId, element: Element);
    fn gui(&mut self) -> &RefCell<GuiState>;

    fn header<S: Into<String>>(&mut self, text: S) {
        let id = self.gui().borrow_mut().fetch_id();
        self.push_element(id, Element::Header(text.into()))
    }

    fn label<T: ToString>(&mut self,value: T) {
        let id = self.gui().borrow_mut().fetch_id();
        self.push_element(id, Element::Label(value.to_string()))
    }

    #[must_use = "The finish method has to be called on the ButtonBuilder to create a button."]
    fn button(&mut self) -> ButtonBuilder<Self> {
        ButtonBuilder {
            parent: self,
            id: None,
            text: None,
        }
    }

    fn layout<'gui>(&'gui mut self) -> Indeterminate<'gui> {
        let id = self.gui().borrow_mut().fetch_id();
        self.push_element(id.clone(), Element::Indeterminate);
        Indeterminate::new(self.gui(), id)
    }
}

// ----------------------------------------------------------------------------
// Element
// ----------------------------------------------------------------------------

#[derive(Debug, PartialEq, Eq, Serialize, Clone)]
enum Element {
    Indeterminate,
    Header(String),
    Label(String),
    Button {
        text: Option<String>,
    },
    StackLayout {
        children: Vec<GuiId>,
    },
    Columns {
        left: GuiId,
        right: GuiId,
    },
}

impl Element {
    fn new_button(text: Option<String>) -> Element {
        Element::Button { 
            text,
        }
    }

    fn to_html(&self, id: &GuiId, state: &GuiState) -> String {
        match self {
            Element::Indeterminate => "".to_owned(),
            Element::Header(text) => format!("<h1>{}</h1>", text),
            Element::Label(value) => format!("<div>{}</div>", value),
            Element::Button { text } => {
                let text = text.clone().unwrap_or_else(|| "Button".to_owned());
                format!("<button>{}</button>", text)
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

#[derive(Debug)]
pub enum Event {
    ButtonPressed(GuiId),
}

#[derive(Debug, Deserialize)]
pub enum BrowserServerEvent {
    ButtonPressed(String)
}

impl Event {
    pub fn from<I: Id>(event: BrowserServerEvent) -> Option<Self> {
        match event {
            BrowserServerEvent::ButtonPressed(identifier) => GuiId::from_str::<I>(&identifier).map(|gui_id| Event::ButtonPressed(gui_id)),
        }
    }
}

/// Json value
#[derive(Debug, Serialize)]
#[serde(transparent)]
struct JsonString(String);

#[derive(Serialize)]
struct ServerBrowserUpdate {
    root: Option<GuiId>,
    added: BTreeMap<GuiId, Element>, // key must be String for serde_json
    removed: Vec<GuiId>,
    updated: BTreeMap<GuiId, Element>, // key must be String for serde_json
}

struct Connection {
    uuid: Uuid,
    to_browser_websocket: Option<WebSocket<TcpStream>>, // This is assigned second
    last_gui: Option<Gui>,
    pending_events: Arc<Mutex<Vec<BrowserServerEvent>>>, // TODO: Not good that this has to be a different type of event
}

impl Connection {
    pub fn events<I: Id>(&mut self) -> Vec<Event> {
        let mut pending_events = self.pending_events.lock().unwrap(); // TODO: unwrap
        mem::take(&mut *pending_events)
            .into_iter()
            .map(|event| Event::from::<I>(event).unwrap()) // TODO: unwrap
            .collect()
    }

    pub fn show_gui(&mut self, gui: Gui) {
        if gui.state.borrow().root.is_none() {
            return;
        }
        let server_browser_update = if let Some(last_gui) = &mut self.last_gui {
            let diff = Gui::diff(last_gui, &gui);
            // TODO: Code duplication
            let added = diff.only_rhs
                .into_iter()
                .map(|gui_id| {
                    let element = gui.state
                        .borrow()
                        .elements
                        .get(&gui_id)
                        .expect("must be available when in diff")
                        .clone();
                    (gui_id, element)
                })
                .collect();
            let updated = diff.unequal
                .into_iter()
                .map(|gui_id| {
                    let element = gui.state
                        .borrow()
                        .elements
                        .get(&gui_id)
                        .expect("must be available when in diff")
                        .clone();
                    (gui_id, element)
                })
                .collect();
            let root = {
                let gui_root = &gui.state.borrow().root;
                let last_root = &last_gui.state.borrow().root;
                if gui_root == last_root {
                    None
                } else {
                    gui_root.clone()
                }
            };
            ServerBrowserUpdate {
                root,
                added,
                removed: diff.only_lhs,
                updated,
            }
        } else {
            let added = gui.state
                .borrow()
                .elements
                .iter()
                .map(|(gui_id, element)| (gui_id.clone(), element.clone()))
                .collect();
            ServerBrowserUpdate {
                root: gui.state.borrow().root.clone(),
                added,
                removed: Vec::new(),
                updated: BTreeMap::new(),
            }
        };
        if let Some(to_browser_websocket) = &mut self.to_browser_websocket {
            let message = serde_json::to_string(&server_browser_update).unwrap(); // TODO: unwrap
            match to_browser_websocket.write_message(Message::Text(message)) {
                Ok(()) => {

                }
                Err(Error::Io(err)) if err.kind() == std::io::ErrorKind::ConnectionAborted => {
                    // Happens when the page is reloaded
                }
                Err(err) => {
                    panic!(err);
                }
            }
        } else {
            // TODO: Error handling
            warn!("Gui ready for sending but no 'to_browser_websocket' found");
        }
        self.last_gui = Some(gui);
    }
}

// TODO: Should have the type parameter "I: Id" because it doesn't make sense
// that a connection would be handled with different id types.
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

const WEBSOCKET_ADDRESS: &'static str = "127.0.0.1:9001";

struct Server {
    connections: Arc<Mutex<Vec<Connection>>>,
}

impl Server {
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
        spawn_incoming_thread(WEBSOCKET_ADDRESS, connections.clone());
        Self { connections }
    }

    pub fn connections<'a>(&mut self) -> Connections {
        let connections = self.connections.lock().unwrap(); // TODO: Error handling
        Connections { r: connections }
    }
}

fn spawn_incoming_thread(address: &'static str, connections: Arc<Mutex<Vec<Connection>>>) {
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

#[derive(Clone, Copy, Deserialize)]
enum WebsocketDirection {
    ToBrowser,
    ToServer,
}

#[derive(Deserialize)]
enum BrowserServerMessage {
    Welcome {
        direction: WebsocketDirection,
        uuid: String,
    },
    Event(BrowserServerEvent),
}

fn handle_incoming_event(
    message: &str,
    connections: Arc<Mutex<Vec<Connection>>>, 
    uuid: Uuid,
) {
    let pending_events = {
        let connections = connections.lock().unwrap(); // TODO: unwrap
        let connection = connections
            .iter()
            .find(|c| c.uuid == uuid);
        if let Some(connection) = connection {
            connection.pending_events.clone()
        } else {
            warn!("Event from browser but to connection found for {}", uuid);
            return;
        }
    };
    match serde_json::from_str::<BrowserServerMessage>(message) {
        Ok(BrowserServerMessage::Event(event)) => {
            info!("Received event: {:?}", event);
            let mut pending_events = pending_events.lock().unwrap();
            pending_events.push(event);
        },
        Ok(BrowserServerMessage::Welcome { .. }) => {
            todo!() // TODO: Error handling
        }
        Err(err) => {
            warn!("Could not deserialize event \"{}\": {}", message, err);
        }
    }
}

fn handle_welcome_message(
    websocket: WebSocket<TcpStream>,
    connections: Arc<Mutex<Vec<Connection>>>, 
    direction: WebsocketDirection, 
    uuid: &str,
) {
    info!("Received welcome message from {}", uuid);
    if let Ok(uuid) = Uuid::parse_str(uuid) {
        match direction {
            WebsocketDirection::ToBrowser => {
                let connection = Connection {
                    to_browser_websocket: Some(websocket),
                    uuid,
                    last_gui: None,
                    pending_events: Arc::new(Mutex::new(Vec::new())),
                };
                let mut connections = connections.lock().unwrap(); // TODO: unwrap
                connections.push(connection);
                let connections_array = connections
                    .iter()
                    .map(|c| c.uuid.to_string())
                    .collect::<Vec<String>>()
                    .join(", ");
                debug!("Connections: {}", format!("[{}]", connections_array));
            }
            WebsocketDirection::ToServer => {
                let mut websocket = websocket;
                loop {
                    match websocket.read_message() {
                        Ok(Message::Text(message)) => handle_incoming_event(&message, connections.clone(), uuid),
                        Ok(unexpected_message) => warn!("Unexpected message: {:?}", unexpected_message),
                        Err(err) => {
                            panic!(err);
                        }
                    }
                }
            }
        }
    } else {
        panic!("Could not parse uuid message in 'welcome' message: {}", uuid);
    }
}

fn handle_incoming_websocket_connection(stream: TcpStream, connections: Arc<Mutex<Vec<Connection>>>) {
    thread::spawn(move || {
        info!("Started websocket connection thread");
        match tungstenite::server::accept(stream) {
            Ok(mut websocket) => {
                match websocket.read_message() {
                    Ok(Message::Text(text)) => {
                        match serde_json::from_str::<BrowserServerMessage>(&text) {
                            Ok(BrowserServerMessage::Welcome { direction, uuid }) => {
                                handle_welcome_message(websocket, connections, direction, &uuid);
                            }
                            Ok(_other) => {
                                todo!()
                            }
                            Err(err) => {
                                panic!(err);
                            }
                        }
                    }
                    Ok(..) => warn!("Unknown message type from websocket"),
                    Err(err) => panic!(err),
                }
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
                let uuid_string = format!("\"{}\"", Uuid::new_v4().to_string());
                let contents = include_str!("../web/index.html").replace("#uuid", &uuid_string);
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
