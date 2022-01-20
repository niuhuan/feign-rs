extern crate proc_macro;

use crate::RequestBody::{Form, Json};
use darling::FromMeta;
use proc_macro::TokenStream;
use proc_macro_error::{abort, proc_macro_error};
use quote::quote;
use syn::spanned::Spanned;
use syn::{parse_macro_input, FnArg};

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
    let args = parse_macro_input!(args as syn::AttributeArgs);
    let input = parse_macro_input!(input as syn::ItemTrait);
    let args: ClientArgs = match ClientArgs::from_list(&args) {
        Ok(v) => v,
        Err(e) => return TokenStream::from(e.write_errors()),
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
            syn::TraitItem::Method(m) => Some(m),
            _ => None,
        })
        .map(|m| gen_method(m));

    let reqwest_client_builder = match args.client_builder {
        Some(builder) => {
            let builder_token: proc_macro2::TokenStream = builder.parse().unwrap();
            quote! {
                Box::new(|| Box::pin(#builder_token()))
            }
        }
        None => quote! {
            Box::new(|| Box::pin(async {
                        Ok(reqwest::ClientBuilder::new().build()?)
                    }))
        },
    };

    let before_send_builder = match args.before_send {
        Some(builder) => {
            let builder_token: proc_macro2::TokenStream = builder.parse().unwrap();
            quote! {
                Some(Box::new(
                    |request_builder: reqwest::RequestBuilder,
                     http_method: HttpMethod,
                     host: String,
                     client_path: String,
                     request_path: String,
                     body: RequestBody,
                     headers: Option<std::collections::HashMap<String, String>>| {
                        Box::pin(#builder_token(
                            request_builder,
                            http_method,
                            host,
                            client_path,
                            request_path,
                            body,
                            headers,
                        ))
                    },
                ))
            }
        }
        None => quote! {
            None
        },
    };

    let tokens = quote! {
        #vis struct #name {
            host: tokio::sync::Mutex<String>,
            path: String,
            reqwest_client_builder: Box<dyn Fn() -> std::pin::Pin<
                Box<dyn Future<
                    Output = Result<reqwest::Client, Box<dyn std::error::Error + Send + Sync>>
                >>
            >>,
            before_send_builder: Option<
                Box<
                    dyn Fn(
                        reqwest::RequestBuilder,
                        feign::HttpMethod,
                        String,
                        String,
                        String,
                        feign::RequestBody,
                        Option<std::collections::HashMap<String, String>>,
                    ) -> std::pin::Pin<
                        Box<
                            dyn Future<
                                Output = Result<
                                    reqwest::RequestBuilder,
                                    Box<dyn std::error::Error + Send + Sync>,
                                >,
                            >,
                        >,
                    >,
                >,
            >,
        }

        impl #name {

            fn new() -> Self{
                Self{
                    host: tokio::sync::Mutex::new(String::from(#base_host)),
                    path: String::from(#base_path),
                    reqwest_client_builder: #reqwest_client_builder,
                    before_send_builder: #before_send_builder,
                }
            }

            async fn configure_host(&self, host: String) {
                let mut lock = self.host.lock().await;
                *lock = host;
            }

            async fn host(&self) -> String {
                format!("{}", self.host.lock().await)
            }

            #(#methods)*
        }
    };

    tokens.into()
}

/// Gen feign methods
fn gen_method(method: &syn::TraitItemMethod) -> proc_macro2::TokenStream {
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

    let _http_method = if let Some(m) = http_method_from_ident(http_method_ident) {
        m
    } else {
        abort!(
            &http_method_ident.span(),
            "Expect one of get, post, put, patch, delete, head."
        )
    };

    let _http_method_token = http_method_to_token(_http_method);

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
    let mut headers = None;

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
            feign::RequestBody::Json(serde_json::to_value(#form)?)
        },
        Some(Json(json)) => quote! {
            feign::RequestBody::Json(serde_json::to_value(#json)?)
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

    quote! {
        pub async fn #name(&self, #inputs) #output {
            let host = self.host().await;
            let client_path = self.path.clone();
            let request_path = String::from(#req_path)#path_variables;
            let url = format!("{}{}{}", host, client_path, request_path);
            let client: reqwest::Client = (self.reqwest_client_builder)().await?;
            let #header_mut req = client
                        .#http_method_ident(url.as_str())
                        #params;
            #headers;
            let req = match Option::as_ref(&self.before_send_builder) {
                Some(before_send_builder) => before_send_builder(
                    req,
                    #_http_method_token,
                    host,
                    client_path,
                    request_path,
                    #request_builder_body,
                    #headers_point,
                ).await?,
                None => req,
            };
            Ok(req
                .send()
                .await?
                .error_for_status()?
                .json()
                .await?)
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
}
