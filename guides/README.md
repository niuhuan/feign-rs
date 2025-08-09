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
- Add serde's dependencies to **Cargo.toml**, because inputs or outputs entities must be **Serialize / Deserialize**

```toml
[dependencies]
feign = "0"
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
          .with_host(String::from("http://127.0.0.1:3001"))
          .build();
}
```

```rust
#[client(path = "/user")]
pub trait UserClient {}

#[tokio::main]
async fn main() {
  let user_client: UserClient = UserClient::builder()
          .with_host_arc(Arc::new(String::from("http://127.0.0.1:3001")))
          .build();
}
```

##### load balance

implement `feign::Host` trait, or use `feign::HostRound`

```rust
let user_client: UserClient = UserClient::builder()
    .with_host(feign::HostRound::new(vec!["http://127.0.0.1:3031".to_string(), "http://127.0.0.1:3032".to_string()]).unwrap())
    .build();
```

### Customer reqwest client builder

Add reqwest to dependencies and enable json feature, or use feign re_exports reqwest.

```toml
reqwest = { version = "0", features = ["json"] }
```

or

```rust
use feign::re_exports::reqwest;
```

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
async fn before_send<Body: Debug>(
    mut request_builder: reqwest::RequestBuilder,
    body: RequestBody<Body>,
    state: &Arc<RwLock<i32>>,
) -> ClientResult<reqwest::RequestBuilder> {
    *state.write().await += 1;

    let (client, request) = request_builder.build_split();
    match request {
        Ok(request) => {
            println!(
                "============= (Before_send)\n\
                    {:?} => {}\n\
                    {:?}\n\
                    {:?}\n\
                    {:?}",
                request.method(),
                request.url().as_str(),
                request.headers(),
                body,
                state,
            );
            request_builder = reqwest::RequestBuilder::from_parts(client, request);
            Ok(request_builder.header("a", "b"))
        }
        Err(err) => Err(err.into()),
    }
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

Optionally set the `State`:

```rust
 let user_client = UserClient::builder()
    ...
    .with_state(Arc::new(RwLock::new(0)))
    .build();
```

Result

```text
============= (Before_send)
GET => 127.0.0.1/user/find_by_id/12
{}
None
user : hello
============= (Before_send)
POST => 127.0.0.1/user/new_user
{"content-type": "application/json"}
Json(User { id: 123, name: "name" })
Some(RwLock { data: 1 })
result : name
```

### Custom deserialize

Add serde_json to Cargo.toml

```toml
serde_json = "1"
```

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
