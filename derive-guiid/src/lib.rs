use proc_macro::TokenStream;
use quote::quote;
use syn::{Data, DataEnum, Fields, Attribute, parse_macro_input, DeriveInput};
use syn::spanned::Spanned;

// #[proc_macro_derive(GuiId, attributes(gui_id))]
#[proc_macro_derive(GuiId)]
pub fn derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let tokens = match input.data {
        Data::Enum(DataEnum{ variants, .. }) => {
            let ty_ident = input.ident;
            quote! {
                impl iwgui::Id for #ty_ident {
                    fn to_string(&self) -> String {
                        todo!()
                    }
                    fn from_str(s: &str) -> Option<Self> {
                        todo!()
                    }
                }
            }
        }
        _ => todo!(),
    };
    tokens.into()
}
