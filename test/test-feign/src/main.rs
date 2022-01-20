use serde_derive::Deserialize;
use serde_derive::Serialize;
use std::collections::HashMap;

use feign::{client, ClientResult, HttpMethod, RequestBody};

async fn client_builder() -> ClientResult<reqwest::Client> {
    Ok(reqwest::ClientBuilder::new().build().unwrap())
}

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

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct User {
    pub id: i64,
    pub name: String,
}

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
    #[get(path = "/headers")]
    async fn headers(
        &self,
        #[json] age: &i64,
        #[headers] headers: HashMap<String, String>,
    ) -> ClientResult<Option<User>>;
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

    let mut headers = HashMap::<String, String>::new();
    headers.insert(String::from("C"), String::from("D"));

    match user_client.headers(&12, headers).await {
        Ok(option) => match option {
            Some(user) => println!("user : {}", user.name),
            None => println!("none"),
        },
        Err(err) => panic!("{}", err),
    };
}
