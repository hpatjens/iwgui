use std::{cell::RefCell, collections::BTreeMap, fmt};
use serde::{Serialize, Deserialize};

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
pub struct GuiDiff {
    pub only_lhs: Vec<GuiId>,
    pub only_rhs: Vec<GuiId>,
    pub unequal: Vec<GuiId>,
}

pub struct Gui {
    state: RefCell<GuiState>,
}

impl<'gui> Gui {
    pub fn empty() -> Self {
        Self {
            state: RefCell::new(GuiState {
                next_id: 0,
                root: None,
                elements: BTreeMap::new(),
            })
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

    pub fn server_browser_update(previous_gui: Option<&Gui>, current_gui: &Gui) -> ServerBrowserUpdate {
        if let Some(previous_gui) = previous_gui {
            let diff = Gui::diff(previous_gui, &current_gui);
            // TODO: Code duplication
            let added = diff.only_rhs
                .into_iter()
                .map(|gui_id| {
                    let element = current_gui.state
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
                    let element = current_gui.state
                        .borrow()
                        .elements
                        .get(&gui_id)
                        .expect("must be available when in diff")
                        .clone();
                    (gui_id, element)
                })
                .collect();
            let root = {
                let gui_root = &current_gui.state.borrow().root;
                let last_root = &previous_gui.state.borrow().root;
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
            let added = current_gui.state
                .borrow()
                .elements
                .iter()
                .map(|(gui_id, element)| (gui_id.clone(), element.clone()))
                .collect();
            ServerBrowserUpdate {
                root: current_gui.state.borrow().root.clone(),
                added,
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

pub struct ButtonBuilder<'parent, P: PushElement> {
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

// TODO: Not all members should be public
pub trait PushElement: Sized {
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
pub struct ServerBrowserUpdate {
    root: Option<GuiId>,
    added: BTreeMap<GuiId, Element>, // key must be String for serde_json
    removed: Vec<GuiId>,
    updated: BTreeMap<GuiId, Element>, // key must be String for serde_json
}
