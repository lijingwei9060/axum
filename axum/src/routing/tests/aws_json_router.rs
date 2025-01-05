use crate::{routing::get, test_helpers::*, AWSJsonRouter};
use axum_core::extract::Request;

use http::StatusCode;

#[crate::test]
async fn routing() {
    let app = AWSJsonRouter::new()
        .route(
            "/users",
            get(|_: Request| async { "users#index" }).post(|_: Request| async { "users#create" }),
        )
        .route("/users/{id}", get(|_: Request| async { "users#show" }))
        .route(
            "/users/{id}/action",
            get(|_: Request| async { "users#action" }),
        );

    let client = TestClient::new(app);

    let res = client.get("/").await;
    println!("{:?}", res);
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}
