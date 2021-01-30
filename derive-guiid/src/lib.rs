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
            let to_string_cases = variants
                .iter()
                .map(|variant| {
                    let variant_ident = &variant.ident;
                    let variant_ident_string = variant_ident.to_string();
                    quote! {
                        #ty_ident::#variant_ident => String::from(#variant_ident_string),
                    }
                })
                .collect::<Vec<_>>();

            let from_str_cases = variants
                .iter()
                .map(|variant| {
                    let variant_ident = &variant.ident;
                    let variant_ident_string = variant_ident.to_string();
                    quote! {
                        #variant_ident_string => Some(#ty_ident::#variant_ident),
                    }
                })
                .collect::<Vec<_>>();

            quote! {
                impl iwgui::Id for #ty_ident {
                    fn to_string(&self) -> String {
                        match self {
                            #(#to_string_cases)*
                        }
                    }
                    fn from_str(s: &str) -> Option<Self> {
                        match s {
                            #(#from_str_cases)*
                            _ => None
                        }
                    }
                }
            }
        }
        _ => todo!(),
    };
    tokens.into()
}
