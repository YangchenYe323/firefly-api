mod api;
mod utils;

use axum::Router;
use axum_cloudflare_adapter::EnvWrapper;
use http::HeaderValue;
use tower_service::Service;
use tracing_subscriber::{fmt::{format::Pretty, time::UtcTime}, layer::SubscriberExt, util::SubscriberInitExt as _};
use tracing_web::{performance_layer, MakeConsoleWriter};
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

    let fmt_layer = tracing_subscriber::fmt::layer()
        .json()
        .with_timer(UtcTime::rfc_3339())
        .with_writer(MakeConsoleWriter);
    let perf_layer = performance_layer().with_details_from_fields(Pretty::default());
    tracing_subscriber::registry()
        .with(fmt_layer)
        .with(perf_layer)
        .init();
}

#[event(fetch)]
async fn fetch(
    req: HttpRequest,
    env: Env,
    _ctx: Context,
) -> Result<axum::http::Response<axum::body::Body>> {
    if req.method() == "OPTIONS" {
        // Allow all origins, methods, and headers
        let mut response = axum::http::Response::new(axum::body::Body::empty());
        response.headers_mut().insert(
            "Access-Control-Allow-Origin",
            HeaderValue::from_str("*").unwrap(),
        );
        response.headers_mut().insert(
            "Access-Control-Allow-Methods",
            HeaderValue::from_str("GET, POST, PUT, DELETE, OPTIONS").unwrap(),
        );
        response.headers_mut().insert(
            "Access-Control-Allow-Headers",
            HeaderValue::from_str("*").unwrap(),
        );
        response.headers_mut().insert(
            "Access-Control-Max-Age",
            HeaderValue::from_str("86400").unwrap(),
        );
        return Ok(response);
    }

    let state = AppState {
        env: EnvWrapper::new(env),
    };

    Ok(router(state).call(req).await?)
}
