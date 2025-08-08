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

    let has_before_send = before_send.is_some();

    let mut req_body = if has_before_send {
        match body {
            None => quote! {
                let req_body = feign::RequestBody::None;
            },
            Some(Form(form)) => quote! {
                let req_body = feign::RequestBody::Form(::feign::re_exports::serde_json::to_value(#form)?);
                req = req.form(#form);
            },
            Some(Json(json)) => quote! {
                let req_body = feign::RequestBody::Json(::feign::re_exports::serde_json::to_value(#json)?);
                req = req.json(#json);
            },
        }
    } else {
        match body {
            None => quote! {},
            Some(Form(form)) => quote! {
                req = req.form(#form);
            },
            Some(Json(json)) => quote! {
                req = req.json(#json);
            },
        }
    };

    let mut headers = if has_before_send {
        match headers {
            None => quote! {
                let mut headers_opt = None;
            },
            Some(headers) => quote! {
                let mut headers_opt = Some(#headers.clone());
                for header in #headers {
                    req = req.header(header.0, header.1);
                }
            },
        }
    } else {
        match headers {
            None => quote! {},
            Some(headers) => quote! {
                    for header in #headers {
                        req = req.header(header.0,header.1);
                    }
            },
        }
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
        if body.is_some() {
            req_body = quote! {
                match #args.body()? {
                    feign::RequestBody::None => {},
                    _ => {
                        return Err(feign::re_exports::anyhow::anyhow!("json or form can only once"));
                    },
                }
            };
        } else {
            req_body = quote! {
                let req_body = #args.body()?;
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
        }
        headers = if has_before_send {
            quote! {
                    #headers
                    if let Some(hs) = #args.headers() {
                        match &mut headers_opt {
                            None => {
                                headers_opt = Some(hs.clone());
                                for header in hs {
                                    req = req.header(header.0, header.1);
                                }
                            },
                            Some(headers) => {
                                return Err(feign::re_exports::anyhow::anyhow!("headers can only once"));
                            }
                        }
                    }
            }
        } else {
            quote! {
                #headers
                if let Some(hs) = #args.headers() {
                    for header in hs {
                        req = req.header(header.0, header.1);
                    }
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
                            #_http_method_token,
                            self.host.host().to_string(),
                            self.path.clone(),
                            request_path.clone(),
                            req_body,
                            headers_opt,
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
    attributes(feigen_path, feigen_query, feigen_json, feigen_form, feigen_headers)
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
                        "feigen_path" => has_path = true,
                        "feigen_query" => has_query = true,
                        "feigen_json" => has_json = true,
                        "feigen_form" => has_form = true,
                        "feigen_headers" => has_headers = true,
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

    let body = match (form_field, json_field) {
        (Some((field_name, _)), None) => quote! {
            feign::RequestBody::Form(::feign::re_exports::serde_json::to_value(&self.#field_name)?)
        },
        (None, Some((field_name, _))) => quote! {
            feign::RequestBody::Json(::feign::re_exports::serde_json::to_value(&self.#field_name)?)
        },
        _ => quote! {feign::RequestBody::None},
    };

    let headers = match headers_field {
        None => quote! {},
        Some((field_name, _)) => quote! {
            Some(&self.#field_name)
        },
    };

    let expanded = quote! {
        impl #name {
            fn path(&self) -> Vec<(&'static str, String)> {
                #path
            }

            fn query(&self) -> Option<Vec<(&'static str, String)>> {
                #query
            }

            fn body(&self) -> feign::ClientResult<feign::RequestBody> {
                Ok(#body)
            }

            fn headers(&self) -> Option<&::std::collections::HashMap<String, String>> {
                #headers
            }
        }
    };

    TokenStream::from(expanded)
}
