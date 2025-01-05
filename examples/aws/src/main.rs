//! Run with
//!
//! ```not_rust
//! cargo run -p aws
//! ```

use axum::{
    extract::{FromRequest, Request},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::post,
    AWSJson, AWSJsonRejection, AWSJsonRouter,
};

use bytes::Bytes;
use serde::{Deserialize, Serialize};

#[tokio::main]
async fn main() {
    let app = AWSJsonRouter::new()
        .route(
            "users",
            post(|AWSJson(user): AWSJson<User>| async move {
                println!("{:?}", user);

                "users#create"
            }),
        )
        .route("users.show", post(|_: Request| async { "users#show" }))
        .route("users.action", post(|_: Request| async { "users#action" }));

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
        .await
        .unwrap();
    println!("listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}

#[derive(Deserialize, Serialize, Debug, Clone)]
struct User {
    first: String,
    last: String,
}

impl<S> FromRequest<S> for User
where
    S: Send + Sync,
{
    type Rejection = AWSJsonRejection;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        match Bytes::from_request(req, state).await {
            Ok(b) => match serde_json::from_slice(&b) {
                Ok(user) => Ok(user),
                Err(e) => Err(AWSJsonRejection::ValidationException(e.into())),
            },
            Err(_) => todo!(),
        }
    }
}
