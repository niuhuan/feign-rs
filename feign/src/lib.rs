use std::fmt::{Debug, Display};
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

pub trait Host : Display + Debug + Sync + Send + 'static{
    fn host(&self) -> &str;
}

impl Host for String {
    fn host(&self) -> &str {
        self.as_str()
    }
}
