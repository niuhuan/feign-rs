<div align="center">

![](images/icon.png)

</div>

<h1 align="center">
Feign-RS (Rest client of Rust)
</h1>

### [Start to use](https://github.com/niuhuan/feign-rs/tree/master/guides)

## Examples

```rust
use serde_derive::Deserialize;
use serde_derive::Serialize;
use std::collections::HashMap;
use feign::{client, ClientResult, Args};

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct User {
    pub id: i64,
    pub name: String,
}

#[derive(Args)]
pub struct PutUserArgs {
    #[arg_path]
    pub id: i64,
    #[arg_query]
    pub q: String,
    #[arg_json]
    pub data: User,
    #[arg_headers]
    pub headers: HashMap<String, String>,
}

#[client(host = "http://127.0.0.1:3000", path = "/user")]
pub trait UserClient {

    #[get(path = "/find_by_id/<id>")]
    async fn find_by_id(&self, #[path] id: i64) -> ClientResult<Option<User>>;

    #[post(path = "/new_user")]
    async fn new_user(&self, #[json] user: &User) -> ClientResult<Option<String>>;

    #[put(path = "/put_user/<id>")]
    async fn put_user(&self, #[args] args: PutUserArgs) -> ClientResult<User>;
}

#[tokio::main]
async fn main() {
    let user_client: UserClient = UserClient::new();

    match user_client.find_by_id(12).await {
        Ok(option) => match option {
            Some(user) => println!("user : {}", user.name),
            None => println!("none"),
        },
        Err(err) => panic!("{}", err),
    };
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
