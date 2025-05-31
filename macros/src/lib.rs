use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{
    Ident, ItemEnum, ItemFn, LitStr, Result, Token,
    parse::{Parse, ParseStream},
    parse_macro_input,
};

struct RequestArgs {
    name: Option<Ident>,
}

impl Parse for RequestArgs {
    fn parse(input: ParseStream) -> Result<Self> {
        let mut name = None;
        while !input.is_empty() {
            let lookahead = input.lookahead1();
            if lookahead.peek(Ident) {
                let ident: Ident = input.parse()?;
                input.parse::<Token![=]>()?;
                let value: LitStr = input.parse()?;
                if ident == "name" {
                    name = Some(Ident::new(&value.value(), value.span()));
                } else {
                    return Err(syn::Error::new_spanned(ident, "Unknown attribute key"));
                }
                if input.peek(Token![,]) {
                    input.parse::<Token![,]>()?;
                }
            } else {
                return Err(lookahead.error());
            }
        }
        Ok(RequestArgs { name })
    }
}

#[proc_macro_attribute]
pub fn request(attr: TokenStream, item: TokenStream) -> TokenStream {
    let args = parse_macro_input!(attr as RequestArgs);
    let input_fn = parse_macro_input!(item as ItemFn);

    let vis = &input_fn.vis;
    let sig = &input_fn.sig;
    let fn_block = &input_fn.block;
    let default_name = &sig.ident;
    let fn_name = format_ident!("__{}", default_name);

    let struct_name = args.name.clone().unwrap_or_else(|| default_name.clone());

    let (arg_names, arg_types): (Vec<_>, Vec<_>) = sig
        .inputs
        .iter()
        .map(|arg| match arg {
            syn::FnArg::Typed(pat_type) => {
                if let syn::Pat::Ident(pat_ident) = &*pat_type.pat {
                    Ok((pat_ident.ident.clone(), &*pat_type.ty))
                } else {
                    Err(syn::Error::new_spanned(
                        &pat_type.pat,
                        "Unsupported argument pattern",
                    ))
                }
            }
            _ => Err(syn::Error::new_spanned(
                arg,
                "Unsupported function argument",
            )),
        })
        .collect::<Result<Vec<_>>>()
        .unwrap()
        .into_iter()
        .unzip();

    let return_type = match &sig.output {
        syn::ReturnType::Type(_, ty) => quote! { #ty },
        syn::ReturnType::Default => quote! { () },
    };

    let expanded = quote! {
        #[allow(non_snake_case)]
        #[warn(non_camel_case_types)]
        #vis async fn #fn_name(#(#arg_names: #arg_types),*) -> #return_type {
            #fn_block
        }

        #[derive(Debug, ::bincode::Encode, ::bincode::Decode, ::serde::Serialize, ::serde::Deserialize)]
        #vis struct #struct_name {
            #(pub #arg_names: #arg_types),*
        }

        #[async_trait::async_trait]
        impl ::protocol::Request for #struct_name {
            type Resp = #return_type;

            async fn handle(self) -> Self::Resp {
                let #struct_name { #(#arg_names),* } = self;
                #fn_name(#(#arg_names),*).await
            }
        }
    };

    TokenStream::from(expanded)
}

struct RpcArgs {
    response: Ident,
}

impl Parse for RpcArgs {
    fn parse(input: ParseStream) -> Result<Self> {
        let mut response = None;
        while !input.is_empty() {
            let ident: Ident = input.parse()?;
            input.parse::<Token![=]>()?;
            let value: LitStr = input.parse()?;
            if ident == "response" {
                response = Some(Ident::new(&value.value(), value.span()));
            } else {
                return Err(syn::Error::new_spanned(ident, "Unknown attribute key"));
            }
            if input.peek(Token![,]) {
                input.parse::<Token![,]>()?;
            }
        }
        match response {
            Some(response) => Ok(RpcArgs { response }),
            None => Err(input.error("Missing required attribute: response")),
        }
    }
}

#[proc_macro_attribute]
pub fn rpc(attr: TokenStream, item: TokenStream) -> TokenStream {
    let args = parse_macro_input!(attr as RpcArgs);
    let input_enum = parse_macro_input!(item as ItemEnum);

    let enum_name = &input_enum.ident;
    let variants = &input_enum.variants;
    let response_name = &args.response;

    let response_variants = variants.iter().map(|v| {
        let variant_name = &v.ident;
        let ty = match &v.fields {
            syn::Fields::Unnamed(fields) if fields.unnamed.len() == 1 => &fields.unnamed[0].ty,
            _ => panic!("Variants must be tuple variants with a single field"),
        };
        quote! {
            #variant_name(<#ty as Request>::Resp)
        }
    });

    let match_arms = variants.iter().map(|v| {
        let variant_name = &v.ident;
        quote! {
            #enum_name::#variant_name(req) => #response_name::#variant_name(req.handle().await),
        }
    });

    let expanded = quote! {
        #[derive(Debug, ::bincode::Encode, ::bincode::Decode, ::serde::Deserialize, ::serde::Serialize)]
        #input_enum

        #[derive(Debug, ::bincode::Encode, ::bincode::Decode, ::serde::Deserialize, ::serde::Serialize)]
        pub enum #response_name {
            #(#response_variants),*
        }

        impl ::protocol::Response for #response_name {}

        #[async_trait::async_trait]
        impl ::protocol::Request for #enum_name {
            type Resp = #response_name;

            async fn handle(self) -> Self::Resp {
                match self {
                    #(#match_arms)*
                }
            }
        }
    };

    TokenStream::from(expanded)
}
