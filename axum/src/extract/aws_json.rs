use std::collections::HashMap;

use axum_core::body::Body;
use axum_core::extract::{FromRequest, Request};
use axum_core::response::{IntoResponse, Response};
use serde::Serialize;

/// AWSJSON Extractor / Response.
#[derive(Debug, Clone, Copy, Default)]
#[must_use]
pub struct AWSJson<T>(pub T);

impl<T, S> FromRequest<S> for AWSJson<T>
where
    T: FromRequest<S>,
    <T as FromRequest<S>>::Rejection: Into<AWSRejection>,
    S: Send + Sync,
{
    type Rejection = AWSRejection;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let t = T::from_request(req, state).await.map_err(Into::into)?;
        Ok(Self(t))
    }
}

/// RejectionContent used for [`AWSJson`].
#[derive(Debug, Serialize)]
pub struct RejectionContent {
    /// type of the rejection.
    #[serde(rename = "__type")]
    pub r#type: String,
    /// same as the r#type.
    pub code: String,
    /// message for the rejection.
    #[serde(rename = "Message")]
    pub message: Option<String>,
    /// content for the rejection.
    #[serde(flatten)]
    pub content: HashMap<String, String>,
}

/// Rejection used for [`AWSJson`].
#[derive(Debug)]
pub struct AWSRejection {
    /// status code for the rejection.
    pub status_code: http::StatusCode,
    /// rejection content for the rejection.
    pub rejection: RejectionContent,
}

impl IntoResponse for AWSRejection {
    fn into_response(self) -> Response {
        http::response::Response::builder()
            .status(self.status_code)
            .header("X-Amzn-Errortype", self.rejection.r#type.clone())
            .body(Body::from(serde_json::to_string(&self.rejection).unwrap()))
            .unwrap()

        // (self.status_code, Json(self.rejection)).into_response()
    }
}

/// The requested operation failed because you provided invalid values for one or more of the request parameters.
/// This exception includes a reason that contains additional information about the violated limit.
pub fn new_invalid_input_exception(
    message: Option<String>,
    reason: Option<String>,
) -> AWSRejection {
    let mut content = HashMap::new();
    if let Some(reason) = reason {
        content.insert("reason".to_string(), reason);
    }
    AWSRejection {
        status_code: http::StatusCode::BAD_REQUEST,
        rejection: RejectionContent {
            r#type: "InvalidInputException".to_string(),
            code: "InvalidInputException".to_string(),
            message,
            content,
        },
    }
}

/// A standard error for input validation failures. This should be thrown by services when a member of the input structure falls outside of the modeled or documented constraints.
pub fn new_validation_exception(
    message: Option<String>,
    field_list: Option<String>,
) -> AWSRejection {
    let mut content = HashMap::new();
    if let Some(field_list) = field_list {
        content.insert("fieldList".to_string(), field_list);
    }
    AWSRejection {
        status_code: http::StatusCode::BAD_REQUEST,
        rejection: RejectionContent {
            r#type: "ValidationException".to_string(),
            code: "ValidationException".to_string(),
            message,
            content: HashMap::new(),
        },
    }
}

/// server can't complete your request because of an internal service error. Try again later.
pub fn new_service_exception(message: Option<String>) -> AWSRejection {
    AWSRejection {
        status_code: http::StatusCode::INTERNAL_SERVER_ERROR,
        rejection: RejectionContent {
            r#type: "ServiceException".to_string(),
            code: "ServiceException".to_string(),
            message,
            content: HashMap::new(),
        },
    }
}

/// You have sent too many requests in too short a period of time. The quota helps protect against denial-of-service attacks. Try again later.
pub fn new_too_many_requests_exception(
    message: Option<String>,
    r#type: Option<String>,
) -> AWSRejection {
    let mut content = HashMap::new();
    if let Some(r#type) = r#type {
        content.insert("Type".to_string(), r#type);
    }
    AWSRejection {
        status_code: http::StatusCode::TOO_MANY_REQUESTS,
        rejection: RejectionContent {
            r#type: "TooManyRequestsException".to_string(),
            code: "TooManyRequestsException".to_string(),
            message,
            content,
        },
    }
}
