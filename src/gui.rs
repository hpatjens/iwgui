use serde::{Deserialize, Serialize};
use std::{cell::RefCell, collections::BTreeMap, panic::Location};
use log::warn;

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Serialize, Deserialize)]
#[serde(transparent)]
pub struct HandleHash(u32);

impl HandleHash {
    fn from_location(location: &Location) -> Self {
        // TODO: Think about this
        let file = fxhash::hash32(location.file());
        let line = fxhash::hash32(&location.line());
        let column = fxhash::hash32(&location.column());
        let hash = fxhash::hash32(&(file ^ line ^ column));
        HandleHash(hash)
    }

    #[track_caller]
    fn from_caller() -> Self {
        Self::from_location(Location::caller())
    }

    fn from_str<S: AsRef<str>>(s: S) -> Self {
        HandleHash(fxhash::hash32(s.as_ref()))
    }

    #[inline]
    fn combine(h1: Self, h2: Self) -> HandleHash {
        HandleHash(fxhash::hash32(&(h1.0 ^ h2.0)))
    }

    #[inline]
    fn combine3(h1: Self, h2: Self, h3: Self) -> HandleHash {
        HandleHash(fxhash::hash32(&(h1.0 ^ h2.0 ^ h3.0)))
    }
}

// ----------------------------------------------------------------------------
// Handle
// ----------------------------------------------------------------------------

pub struct PtrHandle(u32);

impl PtrHandle {
    #[track_caller]
    pub fn new<T>(value: &T) -> Self {
        Self(fxhash::hash32(&(value as *const T)))
    }
}

impl Handle for PtrHandle {
    fn hash(&self) -> HandleHash {
        HandleHash(self.0)
    }
}

pub trait Handle {
    fn hash(&self) -> HandleHash;
}

impl<T> Handle for *const T {
    fn hash(&self) -> HandleHash {
        HandleHash(fxhash::hash32(self))
    }
}

impl Handle for String {
    fn hash(&self) -> HandleHash {
        HandleHash(fxhash::hash32(&self.as_ptr()))
    }
}

impl Handle for usize {
    fn hash(&self) -> HandleHash {
        HandleHash(fxhash::hash32(self))
    }
}

// ----------------------------------------------------------------------------
// GuiState
// ----------------------------------------------------------------------------

#[derive(Debug)]
struct GuiState {
    events: Vec<Event>,
    next_id: usize,
    root: Option<HandleHash>,
    elements: BTreeMap<HandleHash, Element>,
}

impl GuiState {
    fn fetch_id(&mut self) -> usize {
        let result = self.next_id;
        self.next_id += 1;
        result
    }
}

#[derive(Debug)]
pub struct GuiDiff {
    pub only_lhs: Vec<HandleHash>,
    pub only_rhs: Vec<HandleHash>,
    pub unequal: Vec<HandleHash>,
}

#[derive(Debug)]
pub struct Gui {
    state: RefCell<GuiState>,
}

impl<'gui> Gui {
    pub(crate) fn empty(events: Vec<Event>) -> Self {
        Self {
            state: RefCell::new(GuiState {
                events,
                next_id: 0,
                root: None,
                elements: BTreeMap::new(),
            }),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.state.borrow().root.is_none()
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
                Some(_) => {}
            }
        }
        for rhs_id in rhs_state.elements.keys() {
            match lhs_state.elements.get(rhs_id) {
                None => only_rhs.push(rhs_id.clone()),
                Some(_) => {}
            }
        }
        GuiDiff {
            only_lhs,
            only_rhs,
            unequal,
        }
    }

    pub fn server_browser_update(
        previous_gui: Option<&Gui>,
        current_gui: &Gui,
    ) -> ServerBrowserUpdate {
        if let Some(previous_gui) = previous_gui {
            let diff = Gui::diff(previous_gui, &current_gui);
            fn to_tuples(handle_hashes: Vec<HandleHash>, gui: &Gui) -> BTreeMap<HandleHash, Element> {
                handle_hashes
                    .into_iter()
                    .map(|handle_hash| {
                        let element = gui
                            .state
                            .borrow()
                            .elements
                            .get(&handle_hash)
                            .expect("must be available when in diff")
                            .clone();
                        (handle_hash, element)
                    })
                    .collect()
            }
            let added = to_tuples(diff.only_rhs, current_gui);
            let updated = to_tuples(diff.unequal, current_gui);
            let root = {
                let gui_root = &current_gui.state.borrow().root;
                let last_root = &previous_gui.state.borrow().root;
                if gui_root == last_root { None } else { gui_root.clone() }
            };
            ServerBrowserUpdate {
                root,
                added,
                removed: diff.only_lhs,
                updated,
            }
        } else {
            let state = current_gui.state.borrow();
            ServerBrowserUpdate {
                root: state.root.clone(),
                added: state.elements.clone(),
                removed: Vec::new(),
                updated: BTreeMap::new(),
            }
        }
    }

    // TODO: Ensure that this works when called multiple times
    #[track_caller]
    pub fn root(&'gui mut self) -> Indeterminate<'gui> {
        let mut state = self.state.borrow_mut();
        // TODO: Move handle functions into one place
        // TODO: Integrate the hash from the parent
        let handle_hash = HandleHash::from_caller(); 
        state.elements.insert(handle_hash, Element::Indeterminate);
        state.root = Some(handle_hash.clone());
        Indeterminate::new(&self.state, handle_hash)
    }
}

// ----------------------------------------------------------------------------
// Indeterminate
// ----------------------------------------------------------------------------

pub struct Indeterminate<'gui> {
    state: &'gui RefCell<GuiState>,
    handle_hash: HandleHash, // `Element` will be changed when the type of the `Indeterminate` is determined
}

impl<'gui> Indeterminate<'gui> {
    fn new(state: &'gui RefCell<GuiState>, handle_hash: HandleHash) -> Self {
        Self { state, handle_hash }
    }

    pub fn stacklayout(self) -> StackLayout<'gui> {
        let mut state = self.state.borrow_mut();
        let element = Element::StackLayout {
            children: Vec::new(),
        };
        *state
            .elements
            .get_mut(&self.handle_hash)
            .expect("must be inserted") = element;
        StackLayout {
            state: self.state,
            id: self.handle_hash,
        }
    }

    pub fn vertical_panels(self) -> (Indeterminate<'gui>, Indeterminate<'gui>) {
        let mut state = self.state.borrow_mut();
        let left_hash = HandleHash::combine(
            self.handle_hash, 
            HandleHash::from_str(format!("left{}", state.fetch_id())));
        let right_hash = HandleHash::combine(
            self.handle_hash, 
            HandleHash::from_str(format!("right{}", state.fetch_id())));
        state.elements.insert(left_hash, Element::Indeterminate);
        state.elements.insert(right_hash, Element::Indeterminate);
        let target = state.elements.get_mut(&self.handle_hash).expect("must be inserted");
        *target = Element::Columns { left: left_hash, right: right_hash };
        let left = Indeterminate::new(self.state, left_hash);
        let right = Indeterminate::new(self.state, right_hash);
        (left, right)
    }
}

// ----------------------------------------------------------------------------
// StackLayout
// ----------------------------------------------------------------------------

pub struct StackLayout<'gui> {
    state: &'gui RefCell<GuiState>,
    id: HandleHash,
}

impl<'gui> Elements for StackLayout<'gui> {
    fn curve_ball(&mut self) -> CurveBall {
        CurveBall { push_element: self }
    }
}

impl PushElement for StackLayout<'_> {
    fn push_element(&mut self, id: HandleHash, element: Element) {
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

    fn handle_hash(&self) -> HandleHash {
        self.id
    }
}

// ----------------------------------------------------------------------------
// LabelBuilder
// ----------------------------------------------------------------------------

pub struct LabelBuilder<'parent> {
    parent: &'parent mut dyn PushElement,
    id: HandleHash,
    text: String,
}

impl<'parent> LabelBuilder<'parent> {
    fn new(parent: &'parent mut dyn PushElement, id: HandleHash, text: String) -> Self {
        LabelBuilder {
            parent, 
            id,
            text,
        }
    }

    // TODO: Don't create a handle when the builder is create but only either in a `handle` method or in the `finish` method
    #[track_caller]
    pub fn handle<H: Handle>(mut self, handle: &H) -> Self {
        self.id = manual_handle(Location::caller(), handle);
        self
    }

    pub fn finish(self) {
        self.parent.push_element(self.id, Element::Label(self.text));
    }
}

// ----------------------------------------------------------------------------
// ButtonBuilder
// ----------------------------------------------------------------------------

fn manual_handle(location: &Location, handle: &impl Handle) -> HandleHash {
    HandleHash::combine(HandleHash::from_location(location), handle.hash())
}

pub struct ButtonBuilder<'parent> {
    parent: &'parent mut dyn PushElement,
    id: HandleHash,
    text: Option<String>,
}

impl<'parent> ButtonBuilder<'parent> {
    fn new(parent: &'parent mut dyn PushElement, id: HandleHash) -> Self {
        ButtonBuilder {
            parent, 
            id,
            text: None,
        }
    }

    pub fn text<S: AsRef<str>>(mut self, text: S) -> Self {
        self.text = Some(text.as_ref().to_string());
        self
    }

    // TODO: Don't create a handle when the builder is create but only either in a `handle` method or in the `finish` method
    #[track_caller]
    pub fn handle<H: Handle>(mut self, handle: &H) -> Self {
        self.id = manual_handle(Location::caller(), handle);
        self
    }

    pub fn finish(self) -> bool {
        let id = self.id;
        self.parent.push_element(id.clone(), Element::new_button(self.text));
        let events = &mut self.parent.gui().borrow_mut().events;
        let position = events.iter().position(|event| event.id == id);
        if let Some(position) = position {
            events.remove(position);
            true
        } else {
            false
        }
    }
}

// ----------------------------------------------------------------------------
// CheckboxBuilder
// ----------------------------------------------------------------------------

pub struct CheckboxBuilder<'parent, 'value> {
    value: &'value mut bool,
    parent: &'parent mut dyn PushElement,
    id: HandleHash,
    text: Option<String>,
}

impl<'parent, 'value> CheckboxBuilder<'parent, 'value> {
    fn new(parent: &'parent mut dyn PushElement, id: HandleHash, value: &'value mut bool) -> Self {
        CheckboxBuilder {
            value,
            parent,
            id,
            text: None,
        }
    }

    pub fn text<S: ToString>(mut self, text: S) -> Self {
        self.text = Some(text.to_string());
        self
    }

    // TODO: Don't create a handle when the builder is create but only either in a `handle` method or in the `finish` method
    #[track_caller]
    pub fn handle<H: Handle>(mut self, handle: &H) -> Self {
        self.id = manual_handle(Location::caller(), handle);
        self    
    }

    pub fn finish(self) {
        let id = self.id;
        self.parent.push_element(id.clone(), Element::new_checkbox(self.text, *self.value));
        let events = &mut self.parent.gui().borrow_mut().events;
        let event = events.iter().find(|event| event.id == id);
        let position = if let Some(event) = event {
            match event.kind {
                Kind::CheckboxChecked(value) => *self.value = value,
                _ => warn!("wrong event for checkbox {:?}", event),
            }
            events.iter().position(|event| event.id == id)
        } else {
            None
        };
        if let Some(position) = position {
            events.remove(position);
        }
    }
}

// ----------------------------------------------------------------------------
// traits
// ----------------------------------------------------------------------------

pub struct CurveBall<'p> {
    push_element: &'p mut dyn PushElement,
}

trait PushElement {
    fn push_element(&mut self, id: HandleHash, element: Element);
    fn handle_hash(&self) -> HandleHash;
    fn gui(&mut self) -> &RefCell<GuiState>;
}

pub trait Elements {
    #[doc(hidden)]
    fn curve_ball(&mut self) -> CurveBall;

    #[track_caller]
    fn header<S: Into<String>>(&mut self, text: S) {
        let e = self.curve_ball().push_element;
        let id = HandleHash::from_caller();
        e.push_element(id, Element::Header(text.into()))
    }

    #[must_use = "The finish method has to be called on the ButtonBuilder to create a button."]
    #[track_caller]
    fn label<T: AsRef<str>>(&mut self, text: T) -> LabelBuilder {
        let parent = self.curve_ball().push_element;
        let id = HandleHash::from_caller();
        LabelBuilder::new(parent, id, text.as_ref().to_string())
    }

    #[must_use = "The finish method has to be called on the ButtonBuilder to create a button."]
    #[track_caller]
    fn button(&mut self) -> ButtonBuilder {
        let parent = self.curve_ball().push_element;
        let id = HandleHash::from_caller();
        ButtonBuilder::new(parent, id)
    }

    #[must_use = "The finish method has to be called on the ButtonBuilder to create a button."]
    #[track_caller]
    fn checkbox<'value>(&mut self, value: &'value mut bool) -> CheckboxBuilder<'_, 'value> {
        let parent = self.curve_ball().push_element;
        let id = HandleHash::from_caller();
        CheckboxBuilder::new(parent, id, value)
    }

    #[track_caller]
    fn layout<'gui>(&'gui mut self) -> Indeterminate<'gui> {
        let e = self.curve_ball().push_element;
        let handle_hash = HandleHash::combine(
            HandleHash::from_caller(), 
            HandleHash::from_str(e.gui().borrow_mut().fetch_id().to_string()));
        e.push_element(handle_hash, Element::Indeterminate);
        Indeterminate::new(e.gui(), handle_hash)
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
    Button { text: Option<String> },
    Checkbox { text: Option<String>, checked: bool, },
    StackLayout { children: Vec<HandleHash> },
    Columns { left: HandleHash, right: HandleHash },
}

impl Element {
    fn new_button(text: Option<String>) -> Element {
        Element::Button { text }
    }

    fn new_checkbox(text: Option<String>, checked: bool) -> Element {
        Element::Checkbox { text, checked }
    }
}

// ----------------------------------------------------------------------------
//
// ----------------------------------------------------------------------------

#[derive(Debug)]
pub enum Kind {
    None,
    CheckboxChecked(bool),
}

#[derive(Debug)]
pub struct Event {
    id: HandleHash,
    kind: Kind,
}

#[derive(Debug, Deserialize)]
pub enum BrowserServerEvent {
    ButtonPressed(HandleHash),
    CheckboxChecked(HandleHash, bool),
}

impl Event {
    pub fn from(event: BrowserServerEvent) -> Option<Self> {
        match event {
            BrowserServerEvent::ButtonPressed(handle_hash) => {
                Some(Event { 
                    id: handle_hash,
                    kind: Kind::None,
                })
            }
            BrowserServerEvent::CheckboxChecked(handle_hash, state) => {
                Some(Event { 
                    id: handle_hash,
                    kind: Kind::CheckboxChecked(state),
                })
            }
        }
    }
}

/// Json value
#[derive(Debug, Serialize)]
#[serde(transparent)]
struct JsonString(String);

#[derive(Debug, Serialize)]
pub struct ServerBrowserUpdate {
    root: Option<HandleHash>,
    added: BTreeMap<HandleHash, Element>, // key must be String for serde_json
    removed: Vec<HandleHash>,
    updated: BTreeMap<HandleHash, Element>, // key must be String for serde_json
}
