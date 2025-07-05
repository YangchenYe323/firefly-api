mod artwork;
mod song;

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
    /// A 200 response with JSON body
    Ok(String),
    /// A 307 redirect
    TemporaryRedirect(Redirect),
    /// Error response with a status code and message
    Error { status: StatusCode, message: String },
}

impl IntoResponse for ApiV1Response {
    fn into_response(self) -> Response<Body> {
        let mut response = match self {
            Self::Ok(body) => (StatusCode::OK, body).into_response(),
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
            Self::Error { status, message } => (status, message).into_response(),
        };

        // Allow all origins to access the API.
        response.headers_mut().insert(
            "Access-Control-Allow-Origin",
            HeaderValue::from_str("*").unwrap(),
        );
        // Allow all methods to access the API.
        response.headers_mut().insert(
            "Access-Control-Allow-Methods",
            HeaderValue::from_str("GET, POST, PUT, DELETE, OPTIONS").unwrap(),
        );
        // Allow all headers to access the API.
        response.headers_mut().insert(
            "Access-Control-Allow-Headers",
            HeaderValue::from_str("*").unwrap(),
        );
        // Allow caching of access control header values for 1 day.
        response.headers_mut().insert(
            "Access-Control-Max-Age",
            HeaderValue::from_str("86400").unwrap(),
        );

        response
    }
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/artwork", get(artwork::get_artwork))
        .route("/song/search", get(song::search_song))
        .with_state(state)
}
