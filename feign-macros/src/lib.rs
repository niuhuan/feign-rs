extern crate proc_macro;

use crate::Body::{Form, Json};
use darling::FromMeta;
use proc_macro::TokenStream;
use proc_macro_error::{abort, proc_macro_error};
use quote::quote;
use syn::spanned::Spanned;
use syn::{parse_macro_input, FnArg, Pat};

#[proc_macro_error(proc_macro_hack)]
#[proc_macro_attribute]
pub fn client(args: TokenStream, input: TokenStream) -> TokenStream {
    let args = parse_macro_input!(args as syn::AttributeArgs);
    let input = parse_macro_input!(input as syn::ItemTrait);
    let args: ClientArgs = match ClientArgs::from_list(&args) {
        Ok(v) => v,
        Err(e) => return TokenStream::from(e.write_errors()),
    };

    let vis = &input.vis;
    let name = &input.ident;
    let base_url = &args.url;

    let methods = input
        .items
        .iter()
        .filter_map(|item| match item {
            syn::TraitItem::Method(m) => Some(m),
            _ => None,
        })
        .map(|m| gen_method(m, base_url));

    let tokens = quote! {
        #vis struct #name {
        }

        impl #name {

            fn new() -> Self{
                Self{}
            }

            #(#methods)*
        }
    };

    tokens.into()
}

fn gen_method(method: &syn::TraitItemMethod, base_url: &str) -> proc_macro2::TokenStream {
    if method.sig.asyncness.is_none() {
        abort!(
            &method.sig.span(),
            "Non-asynchronous calls are not currently supported"
        )
    }

    let name = &method.sig.ident;
    let inputs = &method.sig.inputs;
    let output = &method.sig.output;
    let attr = method.attrs.iter().next();
    let http_method_ident = match attr.map(|a| a.path.get_ident()).flatten() {
        Some(ident) => ident,
        None => {
            abort!(&method.span(), "Expects an http method")
        }
    };

    let _http_method = if let Some(m) = HttpMethod::from_ident(http_method_ident) {
        m
    } else {
        abort!(
            &http_method_ident.span(),
            "Expect one of get, post, put, patch, delete, head."
        )
    };

    let request: Request = match Request::from_meta(&match attr.unwrap().parse_meta() {
        Ok(a) => a,
        Err(err) => return err.into_compile_error(),
    }) {
        Ok(v) => v,
        Err(err) => return TokenStream::from(err.write_errors()).into(),
    };

    let req_path = &request.path;

    let mut path_variables = Vec::new();
    let mut querys = Vec::new();
    let mut body = None;

    match inputs.first() {
        Some(FnArg::Receiver(_)) => {}
        _ => abort!(&method.sig.span(), "first arg must be &self"),
    };

    inputs
        .iter()
        .filter_map(|fn_arg| match fn_arg {
            FnArg::Receiver(_) => None,
            FnArg::Typed(ty) => Some((ty, &ty.attrs.first()?.path.segments.first()?.ident)),
        })
        .for_each(|(ty, p)| match &*p.to_string() {
            "path" => path_variables.push(&ty.pat),
            "query" => querys.push(&ty.pat),
            "body" => match body {
                None => body = Some(Form(&ty.pat)),
                _ => abort!(&method.sig.span(), "json or form only once"),
            },
            "form" => match body {
                None => body = Some(Json(&ty.pat)),
                _ => abort!(&method.sig.span(), "json or form only once"),
            },
            other => abort!(
                &method.sig.span(),
                format!("not allowed param type : {}", other).as_str()
            ),
        });

    let path_variables = if path_variables.is_empty() {
        quote! {}
    } else {
        let mut stream = proc_macro2::TokenStream::new();
        for pv in path_variables {
            let id = format!("<{}>", quote! {#pv});
            stream.extend(quote! {
                .replace(#id, format!("{}", #pv).as_str())
            });
        }
        stream
    };

    let query = if querys.is_empty() {
        quote! {}
    } else {
        quote! {
            .query(&[#(#querys),*])
        }
    };

    let body = match body {
        None => quote! {},
        Some(Form(form)) => quote! {
            .form(#form)
        },
        Some(Json(json)) => quote! {
            .json(#json)
        },
    };

    let params = quote! {
        #query
        #body
    };

    let inputs = inputs
        .iter()
        .filter_map(|fn_arg| match fn_arg {
            FnArg::Receiver(_) => None,
            FnArg::Typed(a) => {
                let mut a = a.clone();
                a.attrs.clear();
                Some(FnArg::Typed(a))
            }
        })
        .collect::<syn::punctuated::Punctuated<_, syn::Token![,]>>();

    quote! {
        pub async fn #name(&self, #inputs) #output {
            let path = String::from(#req_path)#path_variables;
            let url = format!("{}{}", #base_url, path);
            Ok(reqwest::ClientBuilder::new()
                .build()?
                .#http_method_ident(url.as_str())
                #params
                .send()
                .await?
                .error_for_status()?
                .json()
                .await?)
        }
    }
}

enum HttpMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
    Head,
}

impl HttpMethod {
    fn from_ident(ident: &syn::Ident) -> Option<Self> {
        Some(match &*ident.to_string() {
            "get" => HttpMethod::Get,
            "post" => HttpMethod::Post,
            "put" => HttpMethod::Put,
            "patch" => HttpMethod::Patch,
            "delete" => HttpMethod::Delete,
            "head" => HttpMethod::Head,
            _ => return None,
        })
    }
}

enum Body<'a> {
    Form(&'a Box<Pat>),
    Json(&'a Box<Pat>),
}

#[derive(Debug, FromMeta)]
struct ClientArgs {
    pub url: String,
}

#[derive(Debug, FromMeta)]
struct Request {
    pub path: String,
}
