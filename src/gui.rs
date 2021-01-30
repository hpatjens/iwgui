use fxhash::hash64;
use serde::{Deserialize, Serialize};
use std::{cell::RefCell, collections::BTreeMap, fmt, panic::Location};

pub trait Id: fmt::Debug + Sync + Send + Eq + Ord + Copy {
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

    fn new_handle<S: ToString>(handle_str: S) -> Self {
        GuiId(format!("Handle.{}", handle_str.to_string()))
    }

    // TODO: Maybe use Result with error message
    fn from_str<I: Id>(s: &str) -> Option<Self> {
        const PREFIX_AUTO: &'static str = "Auto.";
        const PREFIX_USER: &'static str = "User.";
        const PREFIX_HANDLE: &'static str = "Handle.";
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
        if s.starts_with(PREFIX_HANDLE) {
            return Some(GuiId::new_handle(&s[PREFIX_HANDLE.len()..]))
        }
        None
    }
}

// ----------------------------------------------------------------------------
// Handle
// ----------------------------------------------------------------------------

pub trait Handle {
    fn hash(&self) -> u64;
}

impl<T> Handle for *const T {
    fn hash(&self) -> u64 {
        fxhash::hash64(self)
    }
}

impl Handle for String {
    fn hash(&self) -> u64 {
        fxhash::hash64(&self.as_ptr())
    }
}

impl Handle for usize {
    fn hash(&self) -> u64 {
        fxhash::hash64(self)
    }
}

// ----------------------------------------------------------------------------
// GuiState
// ----------------------------------------------------------------------------

struct GuiState {
    events: Vec<Event>,
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
pub struct GuiDiff {
    pub only_lhs: Vec<GuiId>,
    pub only_rhs: Vec<GuiId>,
    pub unequal: Vec<GuiId>,
}

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
            fn to_tuples(gui_ids: Vec<GuiId>, gui: &Gui) -> BTreeMap<GuiId, Element> {
                gui_ids
                    .into_iter()
                    .map(|gui_id| {
                        let element = gui
                            .state
                            .borrow()
                            .elements
                            .get(&gui_id)
                            .expect("must be available when in diff")
                            .clone();
                        (gui_id, element)
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
    pub fn root(&'gui mut self) -> Indeterminate<'gui> {
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

pub struct Indeterminate<'gui> {
    state: &'gui RefCell<GuiState>,
    target_id: GuiId,
}

impl<'gui> Indeterminate<'gui> {
    fn new(state: &'gui RefCell<GuiState>, target_id: GuiId) -> Self {
        Self { state, target_id }
    }

    pub fn stacklayout(self) -> StackLayout<'gui> {
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

    pub fn vertical_panels(self) -> (Indeterminate<'gui>, Indeterminate<'gui>) {
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

pub struct StackLayout<'gui> {
    state: &'gui RefCell<GuiState>,
    id: GuiId,
}

impl<'gui> Elements for StackLayout<'gui> {
    fn curve_ball(&mut self) -> CurveBall {
        CurveBall { push_element: self }
    }
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

fn manual_handle(location: &Location, handle: &impl Handle) -> GuiId {
    let caller_hash = handle_from_location(location);
    let handle_hash = handle.hash();
    let handle = fxhash::hash64(&(caller_hash ^ handle_hash));
    GuiId::new_handle(handle)
}

fn manual_handle_from_ptr<T>(location: &Location, value: &T) -> GuiId {
    let caller_hash = handle_from_location(location);
    let handle_hash = fxhash::hash64(&(value as *const T));
    let handle = fxhash::hash64(&(caller_hash ^ handle_hash));
    GuiId::new_handle(handle)
}

pub struct ButtonBuilder<'parent> {
    parent: &'parent mut dyn PushElement,
    id: GuiId,
    text: Option<String>,
}

impl<'parent> ButtonBuilder<'parent> {
    fn new(parent: &'parent mut dyn PushElement, id: GuiId) -> Self {
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

    // TODO: Don't create a handle when the builder is create but only either in a `handle` method or in the `finish` method
    #[track_caller]
    pub fn handle_from_ptr<T>(mut self, value: &T) -> Self {
        self.id = manual_handle_from_ptr(Location::caller(), value);
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

pub struct CheckboxBuilder<'parent> {
    parent: &'parent mut dyn PushElement,
    id: GuiId,
    text: Option<String>,
}

impl<'parent> CheckboxBuilder<'parent> {
    fn new(parent: &'parent mut dyn PushElement, id: GuiId) -> Self {
        CheckboxBuilder {
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

    // TODO: Don't create a handle when the builder is create but only either in a `handle` method or in the `finish` method
    #[track_caller]
    pub fn handle_from_ptr<T>(mut self, value: &T) -> Self {
        self.id = manual_handle_from_ptr(Location::caller(), value);
        self
    }

    pub fn finish(self) {
        self.parent.push_element(self.id, Element::new_checkbox(self.text));
    }
}

// ----------------------------------------------------------------------------
// traits
// ----------------------------------------------------------------------------

pub struct CurveBall<'p> {
    push_element: &'p mut dyn PushElement,
}

trait PushElement {
    fn push_element(&mut self, id: GuiId, element: Element);
    fn gui(&mut self) -> &RefCell<GuiState>;
}

fn handle_from_location(location: &Location) -> u64 {
    let file = fxhash::hash64(location.file());
    let line = fxhash::hash64(&location.line());
    let column = fxhash::hash64(&location.column());
    fxhash::hash64(&(file ^ line ^ column))
}

pub trait Elements {
    #[doc(hidden)]
    fn curve_ball(&mut self) -> CurveBall;

    fn header<S: Into<String>>(&mut self, text: S) {
        let e = self.curve_ball().push_element;
        let id = e.gui().borrow_mut().fetch_id();
        e.push_element(id, Element::Header(text.into()))
    }

    fn label<T: AsRef<str>>(&mut self, value: T) {
        let e = self.curve_ball().push_element;
        let id = e.gui().borrow_mut().fetch_id();
        e.push_element(id, Element::Label(value.as_ref().to_string()))
    }

    #[must_use = "The finish method has to be called on the ButtonBuilder to create a button."]
    #[track_caller]
    fn button(&mut self) -> ButtonBuilder {
        let parent = self.curve_ball().push_element;
        let id = GuiId::new_handle(handle_from_location(&Location::caller()));
        ButtonBuilder::new(parent, id)
    }

    #[must_use = "The finish method has to be called on the ButtonBuilder to create a button."]
    #[track_caller]
    fn checkbox(&mut self) -> CheckboxBuilder {
        let parent = self.curve_ball().push_element;
        let id = GuiId::new_handle(handle_from_location(&Location::caller()));
        CheckboxBuilder::new(parent, id)
    }

    fn layout<'gui>(&'gui mut self) -> Indeterminate<'gui> {
        let e = self.curve_ball().push_element;
        let id = e.gui().borrow_mut().fetch_id();
        e.push_element(id.clone(), Element::Indeterminate);
        Indeterminate::new(e.gui(), id)
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
    Checkbox { text: Option<String> },
    StackLayout { children: Vec<GuiId> },
    Columns { left: GuiId, right: GuiId },
}

impl Element {
    fn new_button(text: Option<String>) -> Element {
        Element::Button { text }
    }

    fn new_checkbox(text: Option<String>) -> Element {
        Element::Checkbox { text }
    }
}

// ----------------------------------------------------------------------------
//
// ----------------------------------------------------------------------------

#[derive(Debug)]
pub struct Event {
    id: GuiId,
}

#[derive(Debug, Deserialize)]
pub enum BrowserServerEvent {
    ButtonPressed(String),
}

impl Event {
    pub fn from<I: Id>(event: BrowserServerEvent) -> Option<Self> {
        match event {
            BrowserServerEvent::ButtonPressed(identifier) => {
                GuiId::from_str::<I>(&identifier).map(|gui_id| Event { id: gui_id })
            }
        }
    }
}

/// Json value
#[derive(Debug, Serialize)]
#[serde(transparent)]
struct JsonString(String);

#[derive(Serialize)]
pub struct ServerBrowserUpdate {
    root: Option<GuiId>,
    added: BTreeMap<GuiId, Element>, // key must be String for serde_json
    removed: Vec<GuiId>,
    updated: BTreeMap<GuiId, Element>, // key must be String for serde_json
}
