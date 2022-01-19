use serde_derive::Deserialize;
use serde_derive::Serialize;

use feign::{client, ClientResult};

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct User {
    pub id: i64,
    pub name: bool,
}

#[client(url = "https://127.0.0.1:3000")]
pub trait UserClient {
    #[get(path = "/find_by_id/<id>")]
    async fn find_by_id(&self, #[path] id: i64) -> ClientResult<Option<User>>;
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
