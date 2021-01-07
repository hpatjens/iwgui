
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
