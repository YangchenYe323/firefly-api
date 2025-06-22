mod artwork;

use axum::{
    body::Body,
    http::{HeaderValue, Response, StatusCode},
    response::{IntoResponse, Redirect},
    routing::get,
    Router,
};

use crate::AppState;

#[derive(Debug)]
pub enum ApiV1Response {
    TemporaryRedirect(Redirect),
    Redirect(Redirect),
    Error { status: StatusCode, message: String },
}

impl IntoResponse for ApiV1Response {
    fn into_response(self) -> Response<Body> {
        match self {
            Self::TemporaryRedirect(redirect) => {
                let mut response = redirect.into_response();
                // We use temporary redirect for things like redirecting to the artwork URL hosted on spotify, we'd like it to be aggresively
                // cached. Set for 30 days for now.
                response.headers_mut().insert(
                    "Cache-Control",
                    HeaderValue::from_str("max-age=2592000").unwrap(),
                );
                response
            }
            Self::Redirect(redirect) => redirect.into_response(),
            Self::Error { status, message } => (status, message).into_response(),
        }
    }
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/artwork", get(artwork::get_artwork))
        .with_state(state)
}
