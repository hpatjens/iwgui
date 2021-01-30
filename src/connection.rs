use log::{debug, error, info, warn};
use serde::Deserialize;
use std::{
    io::{Read, Write},
    mem,
    net::{TcpListener, TcpStream, ToSocketAddrs},
    slice::IterMut,
    sync::Arc,
    thread,
};
use tungstenite::{error::Error, Message, WebSocket};
use uuid::Uuid;
use parking_lot::{Mutex, MutexGuard};

use crate::gui::{BrowserServerEvent, Event, Gui, Id};

pub struct Connection {
    uuid: Uuid,
    to_browser_websocket: Option<WebSocket<TcpStream>>, // This is assigned second
    last_gui: Option<Gui>,
    pending_events: Arc<Mutex<Vec<BrowserServerEvent>>>, // TODO: Not good that this has to be a different type of event
}

impl Connection {
    pub fn gui<I: Id>(&mut self) -> Gui {
        let events = self.events::<I>();
        Gui::empty(events)
    }

    fn events<I: Id>(&mut self) -> Vec<Event> {
        let mut pending_events = self.pending_events.lock();
        mem::take(&mut *pending_events)
            .into_iter()
            .map(|event| Event::from::<I>(event).unwrap()) // TODO: unwrap
            .collect()
    }

    pub fn show_gui(&mut self, gui: Gui) {
        if gui.is_empty() {
            return;
        }
        let server_browser_update = Gui::server_browser_update(self.last_gui.as_ref(), &gui);
        if let Some(to_browser_websocket) = &mut self.to_browser_websocket {
            let message = serde_json::to_string(&server_browser_update).unwrap(); // TODO: unwrap
            match to_browser_websocket.write_message(Message::Text(message)) {
                Ok(()) => {}
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
pub struct Connections<'a> {
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

pub struct Server {
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
        let connections = self.connections.lock();
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

fn handle_incoming_event(message: &str, connections: Arc<Mutex<Vec<Connection>>>, uuid: Uuid) {
    let pending_events = {
        let connections = connections.lock();
        let connection = connections.iter().find(|c| c.uuid == uuid);
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
            let mut pending_events = pending_events.lock();
            pending_events.push(event);
        }
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
                let mut connections = connections.lock();
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
                        Ok(Message::Text(message)) => {
                            handle_incoming_event(&message, connections.clone(), uuid)
                        }
                        Ok(unexpected_message) => {
                            warn!("Unexpected message: {:?}", unexpected_message)
                        }
                        Err(err) => {
                            panic!(err);
                        }
                    }
                }
            }
        }
    } else {
        panic!(
            "Could not parse uuid message in 'welcome' message: {}",
            uuid
        );
    }
}

fn handle_incoming_websocket_connection(
    stream: TcpStream,
    connections: Arc<Mutex<Vec<Connection>>>,
) {
    thread::spawn(move || {
        info!("Started websocket connection thread");
        match tungstenite::server::accept(stream) {
            Ok(mut websocket) => match websocket.read_message() {
                Ok(Message::Text(text)) => {
                    match serde_json::from_str::<BrowserServerMessage>(&text) {
                        Ok(BrowserServerMessage::Welcome { direction, uuid }) => {
                            handle_welcome_message(websocket, connections, direction, &uuid);
                        }
                        Ok(_other) => todo!(),
                        Err(err) => {
                            panic!(err);
                        }
                    }
                }
                Ok(..) => warn!("Unknown message type from websocket"),
                Err(err) => panic!(err),
            },
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
