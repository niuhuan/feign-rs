pub use feign_macros::client;

pub type ClientResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;
