//! Extractors whose rejections use the API error shape.

use axum::body::Bytes;
use axum::extract::{FromRequest, FromRequestParts, Request};
use axum::http::{HeaderMap, StatusCode, header};
use serde::de::DeserializeOwned;

use crate::error::{ApiError, Detail};

#[derive(FromRequestParts)]
#[from_request(via(axum::extract::Path), rejection(ApiError))]
pub struct ApiPath<T>(pub T);

#[derive(FromRequestParts)]
#[from_request(via(axum::extract::Query), rejection(ApiError))]
pub struct ApiQuery<T>(pub T);

/// `application/json`, with optional parameters such as `; charset=utf-8`.
pub(crate) fn is_json(headers: &HeaderMap) -> bool {
    headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(';').next())
        .is_some_and(|mime| mime.trim().eq_ignore_ascii_case("application/json"))
}

/// JSON body. Malformed JSON is a 400; well-formed JSON of the wrong shape is
/// a 422 whose detail names the offending field.
///
/// The `Content-Type` must be `application/json` (415 otherwise). Browsers
/// can send `text/plain` and form bodies cross-site without a CORS preflight,
/// so accepting them would let any page move funds (SEC-01). A body over the
/// size limit is a 413 (SEC-05).
pub struct ApiJson<T>(pub T);

impl<S: Send + Sync, T: DeserializeOwned> FromRequest<S> for ApiJson<T> {
    type Rejection = ApiError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        if !is_json(req.headers()) {
            return Err(ApiError::new(
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                "unsupported_media_type",
                "Content-Type must be application/json",
            ));
        }
        let bytes = Bytes::from_request(req, state).await.map_err(|e| {
            if e.status() == StatusCode::PAYLOAD_TOO_LARGE {
                ApiError::new(
                    StatusCode::PAYLOAD_TOO_LARGE,
                    "payload_too_large",
                    "request body is too large",
                )
            } else {
                ApiError::bad_request(e.body_text())
            }
        })?;
        let de = &mut serde_json::Deserializer::from_slice(&bytes);
        match serde_path_to_error::deserialize(de) {
            Ok(value) => Ok(ApiJson(value)),
            Err(e) => {
                let path = e.path().to_string();
                let inner = e.into_inner();
                if inner.is_data() {
                    let path = if path == "." { String::new() } else { path };
                    Err(ApiError::validation(vec![Detail::new(
                        path,
                        "invalid_value",
                        inner.to_string(),
                    )]))
                } else {
                    Err(ApiError::bad_request(format!("invalid JSON: {inner}")))
                }
            }
        }
    }
}
