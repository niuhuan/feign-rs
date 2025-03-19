pub use anyhow::Result as ClientResult;
pub use feign_macros::*;
use std::fmt::{Debug, Display, Formatter};

pub mod re_exports;
#[cfg(test)]
mod tests;

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

pub trait Host: Display + Debug + Sync + Send + 'static {
    fn host(&self) -> &str;
}

impl Host for String {
    fn host(&self) -> &str {
        self.as_str()
    }
}

pub struct HostRound {
    index: std::sync::Mutex<usize>,
    hosts: Vec<String>,
}

impl HostRound {
    pub fn new(hosts: Vec<String>) -> ClientResult<Self> {
        if hosts.is_empty() {
            return Err(anyhow::anyhow!("HostRound hosts is empty"));
        }
        Ok(HostRound {
            index: std::sync::Mutex::new(0),
            hosts,
        })
    }
}

impl Display for HostRound {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.hosts)
    }
}

impl Debug for HostRound {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HostRound")
            .field("hosts", &self.hosts)
            .finish()
    }
}

impl Host for HostRound {
    fn host(&self) -> &str {
        let mut index = self.index.lock().unwrap();
        let host = self.hosts.get(*index).unwrap();
        *index = (*index + 1) % self.hosts.len();
        host.as_str()
    }
}
