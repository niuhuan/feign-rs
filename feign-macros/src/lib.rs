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
/// # Examlples
///
/// ```
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
    let base_host = &match args.host {
        None => String::from(""),
        Some(value) => value,
    };
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
        #vis struct #name {
            host: std::sync::Arc<dyn feign::Host>,
            path: String,
        }

        impl #name {

            pub fn new() -> Self {
                Self{
                    host: std::sync::Arc::new(String::from(#base_host)),
                    path: String::from(#base_path),
                }
            }

            pub fn new_with_builder(host: std::sync::Arc<dyn feign::Host>) -> Self {
                Self{
                    host,
                    path: String::from(#base_path).into(),
                }
            }

            pub fn builder() -> #builder_name {
                #builder_name::new()
            }

            #(#methods)*
        }

        #vis struct #builder_name {
            host: std::sync::Arc<dyn feign::Host>,
        }

        impl #builder_name {

            pub fn new() -> Self {
                Self{
                    host: std::sync::Arc::new(String::from(#base_host)),
                }
            }

            pub fn build(self) -> #name {
                #name::new_with_builder(self.host)
            }

            pub fn set_host(mut self, host: impl ::feign::Host) -> Self {
                self.host = std::sync::Arc::new(host);
                self
            }

            pub fn set_host_arc(mut self, host: std::sync::Arc<dyn ::feign::Host>) -> Self {
                self.host = host;
                self
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
                _ => abort!(&ty.span(), "json or form only once"),
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

    let query = if querys.is_empty() {
        quote! {}
    } else {
        quote! {
            .query(&[#(#querys),*])
        }
    };

    let req_body = match body {
        None => quote! {},
        Some(Form(form)) => quote! {
            .form(#form)
        },
        Some(Json(json)) => quote! {
            .json(#json)
        },
    };

    let request_builder_body = match body {
        None => quote! {
            feign::RequestBody::None
        },
        Some(Form(form)) => quote! {
            feign::RequestBody::Form(::feign::re_exports::serde_json::to_value(#form)?)
        },
        Some(Json(json)) => quote! {
            feign::RequestBody::Json(::feign::re_exports::serde_json::to_value(#json)?)
        },
    };

    let header_mut: proc_macro2::TokenStream = match headers {
        None => quote! {},
        Some(_) => quote! {mut},
    };

    let headers_point = match headers {
        None => quote! {None},
        Some(headers) => quote! {Some(#headers)},
    };

    let headers = match headers {
        None => quote! {},
        Some(headers) => quote! {
            for header in #headers.clone() {
                req = req.header(header.0,header.1);
            }
        },
    };

    let params = quote! {
        #query
        #req_body
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
                #builder_token(
                            req,
                            #_http_method_token,
                            self.host.host().to_string(),
                            self.path.clone(),
                            request_path.clone(),
                            #request_builder_body,
                            #headers_point,
                        ).await?
            }
        }
        None => quote! {
            req
        },
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
            let request_path = String::from(#req_path)#path_variables;
            let url = format!("{}{}{}", self.host, self.path, request_path);
            let #header_mut req = #reqwest_client_builder
                        .#http_method_ident(url.as_str())
                        #params;
            #headers;
            let req = #before_send_builder;
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
    pub host: Option<String>,
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
