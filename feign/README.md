<h1 align="center">
Feign-RS (Rest client of Rust)
</h1>


## How to use

### Demo server

A server has any restful interface (like this : find_user_by_id, new_user)

```shell
curl 127.1:3000/user/find_by_id/1      
# -> {"id":1,"name":"hello"}

curl -X POST 127.1:3000/user/new_user \
-H 'Content-Type: application/json' \
-d '{"id":1,"name":"Link"}'
# -> "Link"                                                                        ➜  ~ 
```

### Dependencies

- Add feign dependency to **Cargo.toml**
- Add reqwest dependency to **Cargo.toml** and enable feature **json**
- Add serde's dependencies to **Cargo.toml**, because inputs or outputs entities must be **Serialize / Deserialize**

```toml
[dependencies]
feign = "1"
reqwest = { version = "0.11", features = ["json"] }
serde = "1.0"
serde_derive = "1.0"
# runtime
tokio = { version = "1.15", features = ["macros", "rt-multi-thread"] }
```

### Entites

Add a user entity add derives serde_derive::Deserialize and serde_derive::Serialize

```rust
use serde_derive::Deserialize;
use serde_derive::Serialize;

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct User {
    pub id: i64,
    pub name: String,
}
```

### Feign client

- Use feign::client macro and trait make a feign client, host is server address, path is controller context. (In the future, the host can be dynamically replaced and can be ignored)
- In the trait, use the method macro and path args make a request, the member method must async and first arg is recover(&self)
- Use #\[json] / #\[form] post body, use #\[path] for replace \<arg_name> in request path

```rust

use feign::{client, ClientResult};

#[client(host = "http://127.0.0.1:3000", path = "/user")]
pub trait UserClient {
    #[get(path = "/find_by_id/<id>")]
    async fn find_by_id(&self, #[path] id: i64) -> ClientResult<Option<User>>;
    #[post(path = "/new_user")]
    async fn new_user(&self, #[json] user: &User) -> ClientResult<Option<String>>;
}
```

### Demo

Use 

```rust
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

    match user_client
        .new_user(&User {
            id: 123,
            name: "name".to_owned(),
        })
        .await
    {
        Ok(option) => match option {
            Some(result) => println!("result : {}", result),
            None => println!("none"),
        },
        Err(err) => panic!("{}", err),
    };
}
```
```text
user : hello
result : name
```