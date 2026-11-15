//! Extractors whose rejections use the API error shape.

use axum::body::Bytes;
use axum::extract::{FromRequest, FromRequestParts, Request};
use serde::de::DeserializeOwned;

use crate::error::{ApiError, Detail};

#[derive(FromRequestParts)]
#[from_request(via(axum::extract::Path), rejection(ApiError))]
pub struct ApiPath<T>(pub T);

#[derive(FromRequestParts)]
#[from_request(via(axum::extract::Query), rejection(ApiError))]
pub struct ApiQuery<T>(pub T);

/// JSON body. Malformed JSON is a 400; well-formed JSON of the wrong shape is
/// a 422 whose detail names the offending field.
pub struct ApiJson<T>(pub T);

impl<S: Send + Sync, T: DeserializeOwned> FromRequest<S> for ApiJson<T> {
    type Rejection = ApiError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let bytes = Bytes::from_request(req, state)
            .await
            .map_err(|e| ApiError::bad_request(e.body_text()))?;
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
