<h1 align="center">
Feign-RS (Rest client of Rust)
</h1>

### [Start to use](https://github.com/niuhuan/feign-rs/tree/master/guides)

## Examples

```rust
use serde_derive::Deserialize;
use serde_derive::Serialize;

use feign::{client, ClientResult};

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct User {
    pub id: i64,
    pub name: String,
}

#[client(host = "http://127.0.0.1:3000", path = "/user")]
pub trait UserClient {
    
    #[get(path = "/find_by_id/<id>")]
    async fn find_by_id(&self, #[path] id: i64) -> ClientResult<Option<User>>;
    
    #[post(path = "/new_user")]
    async fn new_user(&self, #[json] user: &User) -> ClientResult<Option<String>>;

}
```

## Features

- Easy to use
- Asynchronous request
- Configurable agent
- Supports form, JSON
- Reconfig host
- Additional request processer
- Custom deserializer
