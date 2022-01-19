use warp::Filter;

use serde_derive::Deserialize;
use serde_derive::Serialize;

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct User {
    pub id: i64,
    pub name: String,
}

#[tokio::main]
async fn main() {
    let find_by_id = warp::path!("user" / "find_by_id" / i64).map(|id| {
        serde_json::to_string(&User {
            id,
            name: "hello".to_string(),
        })
        .unwrap()
    });

    let new_user = warp::post()
        .and(warp::path!("user" / "new_user"))
        .and(warp::body::json())
        .map(move |user: User| serde_json::to_string(&user.name).unwrap());

    warp::serve(find_by_id.or(new_user))
        .run(([127, 0, 0, 1], 3000))
        .await;
}
