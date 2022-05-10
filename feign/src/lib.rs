pub use anyhow::Result as ClientResult;
pub use feign_macros::*;

pub mod re_exports;

/// Http methods enumed
#[derive(Debug)]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
    Head,
}

#[derive(Clone, Debug)]
pub enum RequestBody {
    None,
    Json(serde_json::Value),
    Form(serde_json::Value),
}
