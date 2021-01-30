use derive_guiid::GuiId;
use iwgui::Id;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, GuiId)]
enum MyId {
    Button1,
    Button2,
}

fn main() {}

#[test]
fn to_string() {
    assert_eq!(MyId::Button1.to_string(), "Button1");
    assert_eq!(MyId::Button2.to_string(), "Button2");
}

#[test]
fn from_str() {
    assert_eq!(MyId::from_str("Button1"), Some(MyId::Button1));
    assert_eq!(MyId::from_str("Button2"), Some(MyId::Button2));
    assert_eq!(MyId::from_str("Button3"), None);
}