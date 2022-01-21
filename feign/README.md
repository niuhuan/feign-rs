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
feign = "0"
reqwest = { version = "0.11", features = ["json"] }
serde = "1.0"
serde_derive = "1.0"
serde_json = "1.0"
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

- Use feign::client macro and trait make a feign client, host is server address, path is controller context. (The host
  can be dynamically replaced and can be ignored)
- In the trait, use the method macro and path args make a request, the member method must async and first arg is
  recover(&self)
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

## Options

### Put headers

```rust
    #[get(path = "/headers")]
    async fn headers(
        &self,
        #[json] age: &i64,
        #[headers] headers: HashMap<String, String>,
    ) -> ClientResult<Option<User>>;
```

### Dynamic modify host with set_host

```rust
#[client(path = "/user")]
pub trait UserClient {}

#[tokio::main]
async fn main() {
  let user_client: UserClient = UserClient::builder()
          .set_host(String::from("http://127.0.0.1:3001"))
          .build();
}
```

### Customer reqwest client builder

Impl a async fn Result<reqwest::Client, Box<dyn std::error::Error + Sync + Send>>, put fn name to arg client_builder

```rust
use feign::{client, ClientResult};

async fn client_builder() -> ClientResult<reqwest::Client> {
    Ok(reqwest::ClientBuilder::new().build().unwrap())
}

#[client(
host = "http://127.0.0.1:3000",
path = "/user",
client_builder = "client_builder"
)]
pub trait UserClient {}
```

### Customer additional reqwest request builder

#### before_send

If you want check hash of json body, sign to header. Or log the request.

```rust
async fn before_send(
  request_builder: reqwest::RequestBuilder,
  http_method: HttpMethod,
  host: String,
  client_path: String,
  request_path: String,
  body: RequestBody,
  headers: Option<HashMap<String, String>>,
) -> ClientResult<reqwest::RequestBuilder> {
  println!(
    "============= (Before_send)\n\
            {:?} => {}{}{}\n\
            {:?}\n\
            {:?}",
    http_method, host, client_path, request_path, headers, body
  );
  Ok(request_builder.header("a", "b"))
}
```

Set before_send arg with function name

```rust
#[client(
host = "http://127.0.0.1:3000",
path = "/user",
client_builder = "client_builder",
before_send = "before_send"
)]
pub trait UserClient {
    #[get(path = "/find_by_id/<id>")]
    async fn find_by_id(&self, #[path] id: i64) -> ClientResult<Option<User>>;
    #[post(path = "/new_user")]
    async fn new_user(&self, #[json] user: &User) -> ClientResult<Option<String>>;
}
```
Result
```text
============= (Before_send)
Get => http://127.0.0.1:3000/user/find_by_id/12
None
None
============= (Before_send)
Post => http://127.0.0.1:3000/user/new_user
None
Json(Object({"id": Number(123), "name": String("name")}))
```

### Custom deserialize

create async deserializer, result type same as field method, or use generic type.
```rust
async fn bare_string(body: String) -> ClientResult<String> {
    Ok(body)
}

async fn decode<T: for<'de> serde::Deserialize<'de>>(body: String) -> ClientResult<T> {
  Ok(serde_json::from_str(body.as_str())?)
}
```

set deserialize, field method result type same as deserializer
```rust
    #[get(path = "/find_by_id/<id>", deserialize = "decode")]
    async fn find_by_id(&self, #[path] id: i64) -> ClientResult<Option<User>>;

    #[post(path = "/new_user", deserialize = "bare_string")]
    async fn new_user_bare_string(&self, #[json] user: &User) -> ClientResult<String>;
```

```rust
    match user_client
        .new_user_bare_string(&User {
            id: 123,
            name: "name".to_owned(),
        })
        .await
    {
        Ok(result) => println!("result : {}", result),
        Err(err) => panic!("{}", err),
    };
```

result (Raw text)
```text
result : "name"
```

