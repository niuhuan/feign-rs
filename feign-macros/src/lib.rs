extern crate proc_macro;

use crate::RequestBody::{Form, Json};
use darling::ast::NestedMeta;
use darling::{Error, FromMeta};
use proc_macro::TokenStream;
use proc_macro_error::{abort, proc_macro_error};
use quote::quote;
use syn::spanned::Spanned;
use syn::{parse_macro_input, FnArg, TraitItemFn};

/// Make a restful http client
///
/// # Examples
///
/// ```ignore
/// #[client(host = "http://127.0.0.1:3000", path = "/user")]
/// pub trait UserClient {
///     #[get(path = "/find_by_id/<id>")]
///     async fn find_by_id(&self, #[path] id: i64) -> ClientResult<Option<User>>;
///    #[post(path = "/new_user")]
///     async fn new_user(&self, #[json] user: &User) -> Result<Option<String>, Box<dyn std::error::Error>>;
/// }
/// ```
///
#[proc_macro_error]
#[proc_macro_attribute]
pub fn client(args: TokenStream, input: TokenStream) -> TokenStream {
    let args = match NestedMeta::parse_meta_list(args.into()) {
        Ok(v) => v,
        Err(e) => {
            return TokenStream::from(Error::from(e).write_errors());
        }
    };
    let input = parse_macro_input!(input as syn::ItemTrait);
    let args: ClientArgs = match ClientArgs::from_list(&args) {
        Ok(v) => v,
        Err(e) => return TokenStream::from(e.write_errors()),
    };

    let reqwest_client_builder = match args.client_builder {
        Some(builder) => {
            let builder_token: proc_macro2::TokenStream = builder.parse().unwrap();
            quote! {
                #builder_token().await?
            }
        }
        None => quote! {
            ::feign::re_exports::reqwest::ClientBuilder::new().build()?
        },
    };

    let vis = &input.vis;
    let name = &input.ident;
    let base_host = &args.host;
    let base_path = &args.path;

    let methods = input
        .items
        .iter()
        .filter_map(|item| match item {
            syn::TraitItem::Fn(m) => Some(m),
            _ => None,
        })
        .map(|m| gen_method(m, args.before_send.as_ref(), &reqwest_client_builder));

    let builder_name: proc_macro2::TokenStream =
        format!("{}Builder", quote! {#name}).parse().unwrap();

    let tokens = quote! {

        #[derive(Debug)]
        #vis struct #name<T=()> {
            host: std::sync::Arc<dyn feign::Host>,
            path: String,
            state: feign::State<T>,
        }

        impl #name<()> {
            pub fn new() -> #name<()> {
                #name::<()>{
                    host: std::sync::Arc::new(String::from(#base_host)),
                    path: String::from(#base_path),
                    state: feign::State::new(()),
                }
            }

            pub fn builder() -> #builder_name<()> {
                #builder_name::<()>::new()
            }
        }

        impl<T> #name<T> where T: std::any::Any + core::marker::Send + core::marker::Sync + 'static{
            #(#methods)*
        }

        #vis struct #builder_name<T=()>(#name<T>);

        impl #builder_name<()> {
            pub fn new() -> Self {
                Self(#name::<()>::new())
            }
        }

        impl<T> #builder_name<T> {

            pub fn build(self) -> #name<T> {
                self.0
            }

            pub fn with_host(mut self, host: impl feign::Host) -> Self {
                self.with_host_arc(std::sync::Arc::new(host))
            }

            pub fn with_host_arc(mut self, host: std::sync::Arc<dyn ::feign::Host>) -> Self {
                self.0.host = host;
                self
            }

            pub fn with_state<S: std::any::Any + core::marker::Send + core::marker::Sync + 'static>(mut self, state: S) -> #builder_name<S> {
                #builder_name(#name::<S>{
                    host: self.0.host,
                    path: self.0.path,
                    state: feign::State::new(state),
                })
            }
        }
    };

    tokens.into()
}

/// Gen feign methods
fn gen_method(
    method: &TraitItemFn,
    before_send: Option<&String>,
    reqwest_client_builder: &proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
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
    let http_method_ident = match attr.map(|a| a.path().get_ident()).flatten() {
        Some(ident) => ident,
        None => {
            abort!(&method.span(), "Expects an http method")
        }
    };

    let _http_method = if let Some(m) = http_method_from_ident(http_method_ident) {
        m
    } else {
        abort!(
            &http_method_ident.span(),
            "Expect one of get, post, put, patch, delete, head."
        )
    };

    let _http_method_token = http_method_to_token(_http_method);

    let request: Request = match Request::from_meta(&attr.unwrap().meta) {
        Ok(v) => v,
        Err(err) => return TokenStream::from(err.write_errors()).into(),
    };

    let req_path = &request.path;

    let mut path_variables = Vec::new();
    let mut querys = Vec::new();
    let mut body = None;
    let mut headers = None;
    let mut args = None;

    match inputs.first() {
        Some(FnArg::Receiver(_)) => {}
        _ => abort!(&method.sig.span(), "first arg must be &self"),
    };

    inputs
        .iter()
        .filter_map(|fn_arg| match fn_arg {
            FnArg::Receiver(_) => None,
            FnArg::Typed(ty) => Some((ty, &ty.attrs.first()?.path().segments.first()?.ident)),
        })
        .for_each(|(ty, p)| match &*p.to_string() {
            "path" => path_variables.push(&ty.pat),
            "query" => querys.push(&ty.pat),
            "json" => match body {
                None => body = Some(Json(&ty.pat)),
                _ => abort!(&ty.span(), "json or form only once"),
            },
            "form" => match body {
                None => body = Some(Form(&ty.pat)),
                _ => abort!(&ty.span(), "json or form only once"),
            },
            "headers" => match headers {
                None => headers = Some(&ty.pat),
                _ => abort!(&ty.span(), "headers only once"),
            },
            "args" => match args {
                None => args = Some(&ty.pat),
                _ => abort!(&ty.span(), "args only once"),
            },
            other => abort!(
                &ty.span(),
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

    let mut query = if querys.is_empty() {
        quote! {}
    } else {
        quote! {
            req = req.query(&[#(#querys),*]);
        }
    };

    let (mut req_body, mut req_body_enum) = match body {
        None => (quote! {}, quote! {feign::RequestBody::<()>::None}),
        Some(Form(form)) => (
            quote! {
                req = req.form(#form);
            },
            quote! {feign::RequestBody::Form(#form)},
        ),
        Some(Json(json)) => (
            quote! {
                req = req.json(#json);
            },
            quote! {feign::RequestBody::Json(#json)},
        ),
    };

    let mut headers = match headers {
        None => quote! {},
        Some(headers) => quote! {
                for header in #headers {
                    req = req.header(header.0,header.1);
                }
        },
    };

    let mut args_path = quote! {};
    if let Some(args) = args {
        args_path = quote! {
            for path in #args.path() {
                request_path = request_path.replace(path.0, path.1.as_str());
            }
        };
        query = quote! {
            if let Some(query) = #args.query() {
                req = req.query(&query);
            }
        };
        // allready has req_body
        if body.is_some() {
            req_body = quote! {
                #req_body
                match #args.body() {
                    feign::RequestBody::None => {},
                    _ => {
                        return Err(feign::re_exports::anyhow::anyhow!("json or form can only once"));
                    },
                }
            };
        } else {
            req_body = quote! {
                let req_body = #args.body();
                match &req_body {
                    feign::RequestBody::None => {},
                    feign::RequestBody::Form(form) => {
                        req = req.form(form);
                    },
                    feign::RequestBody::Json(json) => {
                        req = req.json(json);
                    },
                }
            };
            req_body_enum = quote! {req_body};
        }
        headers = quote! {
            #headers
            if let Some(headers) = #args.headers() {
                for header in headers {
                    req = req.header(header.0, header.1);
                }
            }
        };
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

    let before_send_builder = match before_send {
        Some(builder) => {
            let builder_token: proc_macro2::TokenStream = builder.clone().parse().unwrap();
            quote! {
                let req = #builder_token(
                            req,
                            #req_body_enum,
                            self.state.downcast_ref(),
                        ).await?;
            }
        }
        None => quote! {},
    };

    let deserialize = match request.deserialize {
        None => quote! {::feign::re_exports::serde_json::from_str(text.as_str())},
        Some(deserialize) => {
            let builder_token: proc_macro2::TokenStream = deserialize.parse().unwrap();
            quote! {#builder_token(text).await}
        }
    };

    quote! {
        pub async fn #name(&self, #inputs) #output {
            let mut request_path = String::from(#req_path)#path_variables;
            #args_path
            let url = format!("{}{}{}", self.host, self.path, request_path);
            let mut req = #reqwest_client_builder
                        .#http_method_ident(url.as_str());
            #query
            #req_body
            #headers
            #before_send_builder
            let text = req
                .send()
                .await?
                .error_for_status()?
                .text()
                .await?;
            Ok(#deserialize?)
        }
    }
}

/// Http methods enumed
enum HttpMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
    Head,
}

fn http_method_from_ident(ident: &syn::Ident) -> Option<HttpMethod> {
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

fn http_method_to_token(method: HttpMethod) -> proc_macro2::TokenStream {
    match method {
        HttpMethod::Get => "feign::HttpMethod::Get",
        HttpMethod::Post => "feign::HttpMethod::Post",
        HttpMethod::Put => "feign::HttpMethod::Put",
        HttpMethod::Patch => "feign::HttpMethod::Patch",
        HttpMethod::Delete => "feign::HttpMethod::Delete",
        HttpMethod::Head => "feign::HttpMethod::Head",
    }
    .parse()
    .unwrap()
}

/// body types
enum RequestBody<'a> {
    Form(&'a Box<syn::Pat>),
    Json(&'a Box<syn::Pat>),
}

/// Args of client
#[derive(Debug, FromMeta)]
struct ClientArgs {
    #[darling(default)]
    pub host: String,
    #[darling(default)]
    pub path: String,
    #[darling(default)]
    pub client_builder: Option<String>,
    #[darling(default)]
    pub before_send: Option<String>,
}

/// Args of request
#[derive(Debug, FromMeta)]
struct Request {
    pub path: String,
    #[darling(default)]
    pub deserialize: Option<String>,
}

/// Derive macro for the `Args` trait
///
/// This macro automatically implements the `Args` trait for a struct,
/// providing implementations for `request_path` and `request_builder` methods
/// based on field attributes like `#[path]`, `#[query]`, `#[json]`, `#[form]`, `#[headers]`.
///
/// # Examples
///
/// ```ignore
/// use feign::Args;
///
/// #[derive(Args)]
/// struct MyArgs {
///     #[path]
///     pub id: i64,
///     #[query]
///     pub name: String,
///     #[json]
///     pub data: UserData,
///     #[headers]
///     pub auth: String,
/// }
/// ```
#[proc_macro_error]
#[proc_macro_derive(
    Args,
    attributes(feign_path, feign_query, feign_json, feign_form, feign_headers)
)]
pub fn derive_args(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as syn::DeriveInput);
    let name = &input.ident;

    let fields = match &input.data {
        syn::Data::Struct(data) => &data.fields,
        _ => abort!(
            &input.ident.span(),
            "Args derive macro only supports structs"
        ),
    };

    let mut path_fields: Vec<(&syn::Ident, &syn::Type)> = Vec::new();
    let mut query_fields: Vec<(&syn::Ident, &syn::Type)> = Vec::new();
    let mut json_field: Option<(&syn::Ident, &syn::Type)> = None;
    let mut form_field: Option<(&syn::Ident, &syn::Type)> = None;
    let mut headers_field: Option<(&syn::Ident, &syn::Type)> = None;

    for field in fields.iter() {
        let field_name = field.ident.as_ref().unwrap();
        let field_type = &field.ty;

        let mut has_path = false;
        let mut has_query = false;
        let mut has_json = false;
        let mut has_form = false;
        let mut has_headers = false;

        for attr in &field.attrs {
            if let syn::Meta::Path(path) = &attr.meta {
                if let Some(ident) = path.get_ident() {
                    match ident.to_string().as_str() {
                        "feign_path" => has_path = true,
                        "feign_query" => has_query = true,
                        "feign_json" => has_json = true,
                        "feign_form" => has_form = true,
                        "feign_headers" => has_headers = true,
                        _ => {}
                    }
                }
            }
        }

        if has_path {
            path_fields.push((field_name, field_type));
        } else if has_query {
            query_fields.push((field_name, field_type));
        } else if has_json {
            match json_field {
                None => json_field = Some((field_name, field_type)),
                _ => abort!(&field.span(), "json only once"),
            }
        } else if has_form {
            match form_field {
                None => form_field = Some((field_name, field_type)),
                _ => abort!(&field.span(), "form only once"),
            }
        } else if has_headers {
            match headers_field {
                None => headers_field = Some((field_name, field_type)),
                _ => abort!(&field.span(), "headers only once"),
            }
        }
    }

    if json_field.is_some() && form_field.is_some() {
        abort!(&fields.span(), "json or form only once");
    }

    // Generate request_path method
    let path = if path_fields.is_empty() {
        quote! {}
    } else {
        let path_pairs: Vec<_> = path_fields
            .iter()
            .map(|(field_name, _)| {
                let id = format!("<{}>", field_name);
                quote! {
                    (#id, format!("{}", self.#field_name))
                }
            })
            .collect();
        quote! {
            vec![#(#path_pairs),*]
        }
    };

    // Generate request_builder method
    let query = if query_fields.is_empty() {
        quote! {None}
    } else {
        let query_pairs: Vec<_> = query_fields
            .iter()
            .map(|(field_name, _)| {
                quote! {
                    (stringify!(#field_name), format!("{}", self.#field_name))
                }
            })
            .collect();
        quote! {
            Some(vec![#(#query_pairs),*])
        }
    };

    let (body, body_type) = match (form_field, json_field) {
        (Some((field_name, ty)), None) => (
            quote! {
                feign::RequestBody::Form(&self.#field_name)
            },
            quote! {feign::RequestBody<&#ty>},
        ),
        (None, Some((field_name, ty))) => (
            quote! {
                feign::RequestBody::Json(&self.#field_name)
            },
            quote! {feign::RequestBody<&#ty>},
        ),
        _ => (
            quote! {feign::RequestBody::<()>::None},
            quote! {feign::RequestBody<()>},
        ),
    };

    let (headers, headers_type) = match headers_field {
        None => (
            quote! {None},
            quote! {Option<&std::collections::HashMap<String, String>>},
        ),
        Some((field_name, ty)) => (
            quote! {
                Some(&self.#field_name)
            },
            quote! {Option<&#ty>},
        ),
    };

    let expanded = quote! {
        impl #name {
            fn path(&self) -> Vec<(&'static str, String)> {
                #path
            }

            fn query(&self) -> Option<Vec<(&'static str, String)>> {
                #query
            }

            fn body(&self) -> #body_type {
                #body
            }

            fn headers(&self) -> #headers_type {
                #headers
            }
        }
    };

    TokenStream::from(expanded)
}
