pub use feign_macros::*;

/// Result
pub type ClientResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

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
