use derive_guiid::GuiId;

#[derive(GuiId)]
enum MyId {
    Button1,
    Button2,
}

fn main() {}

#[test]
fn test1() {
    assert_eq!(MyId::Button1.to_string(), "Button1");
    assert_eq!(MyId::Button2.to_string(), "Button2");
}