mod api;

use std::collections::HashMap;

use axum::Router;
use axum_cloudflare_adapter::EnvWrapper;
use http::HeaderValue;
use tower_service::Service;
use worker::*;

#[derive(Clone)]
struct AppState {
    env: EnvWrapper,
}

fn router(state: AppState) -> Router {
    Router::new().nest("/api/v1", api::v1::router(state))
}

#[event(start)]
fn start() {
    console_error_panic_hook::set_once();
}

#[event(fetch)]
async fn fetch(
    req: HttpRequest,
    env: Env,
    _ctx: Context,
) -> Result<axum::http::Response<axum::body::Body>> {
    let state = AppState {
        env: EnvWrapper::new(env),
    };

    if req.method() == "OPTIONS" {
        let mut response = axum::http::Response::new(axum::body::Body::empty());
        response.headers_mut().insert("Access-Control-Allow-Origin", HeaderValue::from_str("*").unwrap());
        response.headers_mut().insert("Access-Control-Allow-Methods", HeaderValue::from_str("GET, POST, PUT, DELETE, OPTIONS").unwrap());
        response.headers_mut().insert("Access-Control-Allow-Headers", HeaderValue::from_str("*").unwrap());
        response.headers_mut().insert("Access-Control-Max-Age", HeaderValue::from_str("86400").unwrap());
        return Ok(response);
    }


    Ok(router(state).call(req).await?)
}
