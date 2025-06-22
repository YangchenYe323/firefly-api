mod api;

use axum::Router;
use axum_cloudflare_adapter::EnvWrapper;
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

    Ok(router(state).call(req).await?)
}
