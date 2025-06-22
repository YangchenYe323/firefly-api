use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::Redirect,
};
use axum_cloudflare_adapter::wasm_compat;
use rspotify::{
    model::{SearchResult, SearchType},
    prelude::BaseClient,
};
use serde::Deserialize;

use crate::{
    api::v1::{ApiV1Response},
    AppState,
};

#[derive(Debug, Deserialize)]
pub enum ArtworkSize {
    #[serde(rename = "small")]
    Small,
    #[serde(rename = "medium")]
    Medium,
    #[serde(rename = "large")]
    Large,
}

impl ArtworkSize {
    pub fn to_size(&self) -> (u32, u32) {
        match self {
            ArtworkSize::Small => (64, 64),
            ArtworkSize::Medium => (300, 300),
            ArtworkSize::Large => (640, 640),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct GetArtworkQuery {
    title: String,
    artist: String,
    size: ArtworkSize,
}

/// This is a cloudflare worker API for retrieving album artwork picture.
///
/// It effectively acts as a caching proxy for the Spotify Web API.
///
/// The query parameters are:
/// - title: The title of the album.
/// - artist: The artist of the album.
/// - size: The size of the artwork.
///
/// The size can be one of the following:
/// - small: 64x64
/// - medium: 300x300
/// - large: 640x640
///
/// # Returns
/// - Redirect to the artwork URL if the search is successful.
/// - 404 if no track is found for the given query.
/// - 500 if there is an error with the Spotify API.
///
#[wasm_compat]
pub async fn get_artwork(
    State(state): State<AppState>,
    Query(query): Query<GetArtworkQuery>,
) -> ApiV1Response {
    let GetArtworkQuery { title, artist, size } = query;

    let credential = {
        let Ok(client_id) = state.env.env.var("SPOTIFY_WEB_API_CLIENT_ID") else {
            return ApiV1Response::Error {
                status: StatusCode::INTERNAL_SERVER_ERROR,
                message: "Failed to get client ID".to_string(),
            };
        };

        let Ok(client_secret) = state.env.env.var("SPOTIFY_WEB_API_CLIENT_SECRET") else {
            return ApiV1Response::Error {
                status: StatusCode::INTERNAL_SERVER_ERROR,
                message: "Failed to get client secret".to_string(),
            };
        };

        let client_id = client_id.to_string();
        let client_secret = client_secret.to_string();

        rspotify::Credentials::new(&client_id, &client_secret)
    };

    let spotify = rspotify::ClientCredsSpotify::new(credential);
    match spotify.request_token().await {
        Ok(_) => (),
        Err(e) => {
            return ApiV1Response::Error {
                status: StatusCode::INTERNAL_SERVER_ERROR,
                message: format!("Failed to request token: {}", e),
            };
        }
    };

    let query = format!("track:{} artist:{}", title, artist);

    let search_result = match spotify
        .search(&query, SearchType::Track, None, None, None, None)
        .await
    {
        Ok(search_result) => search_result,
        Err(e) => {
            return ApiV1Response::Error {
                status: StatusCode::INTERNAL_SERVER_ERROR,
                message: format!("Failed to search for artwork: {}", e),
            };
        }
    };

    match search_result {
        SearchResult::Tracks(tracks) => {
            let Some(track) = tracks.items.first() else {
                // This could be expected. Might be a bad query.
                return ApiV1Response::Error {
                    status: StatusCode::NOT_FOUND,
                    message: format!(
                        "No track found for song with title: {} and artist: {}",
                        title, artist
                    ),
                };
            };

            let (width, height) = size.to_size();

            // Find the image with the specified size
            let Some(image) = track
                .album
                .images
                .iter()
                .find(|image| image.width == Some(width) && image.height == Some(height))
            else {

                return ApiV1Response::Error {
                    status: StatusCode::INTERNAL_SERVER_ERROR,
                    message: "No image found".to_string(),
                };
            };

            ApiV1Response::TemporaryRedirect(Redirect::temporary(&image.url))
        }
        r => {
            return ApiV1Response::Error {
                status: StatusCode::INTERNAL_SERVER_ERROR,
                message: format!("Unexpected search result: {:?}", r),
            };
        }
    }
}
