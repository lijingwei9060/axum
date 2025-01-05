use axum_core::__composite_rejection as composite_rejection;
use axum_core::__define_rejection as define_rejection;
use axum_core::extract::{FromRequest, Request};

/// AWSJSON Extractor / Response.
#[derive(Debug, Clone, Copy, Default)]
#[must_use]
pub struct AWSJson<T>(pub T);

impl<T, S> FromRequest<S> for AWSJson<T>
where
    T: FromRequest<S>,
    <T as FromRequest<S>>::Rejection: Into<AWSJsonRejection>,
    S: Send + Sync,
{
    type Rejection = AWSJsonRejection;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let t = T::from_request(req, state).await.map_err(Into::into)?;
        Ok(Self(t))
    }
}

/// Rejection used for [`AWSJson`].
///
/// Contains one variant for each way the [`AWSJson`] extractor
/// can fail.
#[derive(Debug)]
pub enum AWSJsonRejection {
    /// Rejection type for [`AWSJsonRejection`] used if the `Content-Type`
    /// header is missing.
    MissingAWSJsonContentType,
    /// Rejection type for [`AWSJsonRejection`].
    ///
    /// This rejection is used if the request body is syntactically valid JSON but couldn't be
    /// deserialized into the target type.
    ValidationException(Box<dyn std::error::Error + Send + Sync>),
}

impl AWSJsonRejection {
    /// Get the status code used for this rejection.
    pub fn status(&self) -> http::StatusCode {
        match self {
            AWSJsonRejection::MissingAWSJsonContentType => http::StatusCode::UNSUPPORTED_MEDIA_TYPE,
            AWSJsonRejection::ValidationException(_) => http::StatusCode::BAD_REQUEST,
        }
    }

    /// Get the response body text used for this rejection.
    pub fn body_text(&self) -> String {
        match self {
            AWSJsonRejection::MissingAWSJsonContentType => {
                "Missing Content-Type: application/json".to_string()
            }
            AWSJsonRejection::ValidationException(_) => "Invalid JSON".to_string(),
        }
    }
}

impl crate::response::IntoResponse for AWSJsonRejection {
    fn into_response(self) -> crate::response::Response {
        match self {
            AWSJsonRejection::MissingAWSJsonContentType => (
                http::StatusCode::UNSUPPORTED_MEDIA_TYPE,
                "Missing Content-Type: application/json",
            )
                .into_response(),
            AWSJsonRejection::ValidationException(_) => {
                (http::StatusCode::BAD_REQUEST, "Invalid JSON").into_response()
            }
        }
    }
}
