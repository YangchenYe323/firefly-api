use axum::extract::Query;
use crate::api::v1::ApiV1Response;
use axum_cloudflare_adapter::wasm_compat;
use base64::Engine;
use http::{HeaderMap, HeaderValue, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Deserialize)]
pub struct SearchSongQuery {
    title: String,
}

#[derive(Debug, Serialize)]
pub struct Songs {
    /// A list of songs that match the searched title.
    songs: Vec<Song>,
}

/// Describes info about a song searched from the API.
#[derive(Debug, Serialize)]
pub struct Song {
    /// The title of the song.
    title: String,
    /// The artist of the song.
    artist: String,
    /// The album of the song.
    album: String,
    /// The fragment of the lyrics (the first five lines)
    lyrics_fragment: String,
}

/// Cloudflare worker API for searching song information
/// 
/// The query parameters are:
/// - title: The title of the song to search for.
/// 
/// # Returns
/// 
/// - 200 OK: A JSON object containing a list of songs that match the searched title.
/// - 400 Bad Request: If the title is empty.
/// - 500 Internal Server Error: If there is an error with the QQ Music API.
#[wasm_compat]
pub async fn search_song(Query(query): Query<SearchSongQuery>) -> ApiV1Response {
    let songs = match qq_music_search_song(query).await {
        Ok(songs) => songs,
        Err(e) => return ApiV1Response::Error {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: e.to_string(),
        },
    };
    ApiV1Response::Ok(serde_json::to_string(&songs).unwrap())
}

/// Search for a song using the QQ Music API. Thank goes to https://github.com/Rain120/qq-music-api
/// for the API specification.
/// We need this because the Spotify API doesn't provide lyrics for songs.
async fn qq_music_search_song(query: SearchSongQuery) -> anyhow::Result<Songs> {
    let cli = {
        let mut default_headers = HeaderMap::new();
        default_headers.insert(
                "User-Agent",
                HeaderValue::from_str("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/125.0.0.0 Safari/537.36").expect("Failed to parse user agent"),
            );
        default_headers.insert(
            "Referer",
            HeaderValue::from_str("https://c.y.qq.com/").expect("Failed to parse referer"),
        );
        default_headers.insert(
            "Host",
            HeaderValue::from_str("c.y.qq.com").expect("Failed to parse host"),
        );

        let cli = reqwest::Client::builder()
            .default_headers(default_headers)
            .build()
            .expect("Failed to build client");
        cli
    };

    // First, we do a keyword search using the title
    let search_response = cli
        .get("https://c.y.qq.com/soso/fcgi-bin/client_search_cp")
        .query(&[
            ("format", "json"),
            ("outCharset", "utf-8"),
            ("ct", "24"),
            ("qqmusic_ver", "1298"),
            ("remoteplace", "txt.yqq.song"),
            ("t", "0"),
            ("aggr", "1"),
            ("cr", "1"),
            ("lossless", "0"),
            ("flag_qc", "0"),
            ("platform", "yqq.json"),
            ("w", &query.title),
            ("g_tk", "1124214810"),
            ("loginUin", "0"),
            ("hostUin", "0"),
            ("inCharset", "utf8"),
            ("outCharset", "utf-8"),
            ("notice", "0"),
            ("needNewCode", "0"),
        ])
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to send search request: {}", e))?
        .error_for_status()?;

    let search_result: Value = search_response.json().await?;
    let data = search_result["data"]["song"]["list"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("Failed to parse search API response"))?;

    // Only take the first 3 results.
    let len = if data.len() > 3 { 3 } else { data.len() };

    let mut songs = Vec::with_capacity(len);

    for song_data in data.into_iter().take(len) {
        let title = song_data["songname"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Failed to parse song title"))?
            .to_string();
        let artist = song_data["singer"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("Failed to parse song artist"))?
            .first()
            .map_or("", |singer| singer["name"].as_str().unwrap_or_default())
            .to_string();
        let album = song_data["albumname"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Failed to parse song album"))?
            .to_string();
        let songmid = song_data["songmid"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Failed to parse song mid"))?;
        let lyrics_response = cli
            .get("https://c.y.qq.com/lyric/fcgi-bin/fcg_query_lyric_new.fcg")
            .query(&[
                ("format", "json"),
                ("outCharset", "utf-8"),
                ("songmid", songmid),
            ])
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to send lyrics request: {}", e))?
            .error_for_status()?;

        let lyrics_result: Value = lyrics_response.json().await.map_err(|e| anyhow::anyhow!("Failed to parse lyrics response: {}", e))?;
        let lyrics_base64= lyrics_result["lyric"].as_str().ok_or_else(|| anyhow::anyhow!("Failed to parse lyrics"))?;
        // Fetch lyrics
        let lyrics_fragment = decode_and_format_lyrics(&title, lyrics_base64)?;

        songs.push(Song {
            title,
            artist,
            album,
            lyrics_fragment,
        });
    }

    Ok(Songs { songs })
}

fn decode_and_format_lyrics(title: &str, raw_base64_lyrics: &str) -> anyhow::Result<String> {
    let decoded = base64::engine::general_purpose::STANDARD.decode(raw_base64_lyrics).map_err(|e| anyhow::anyhow!("Failed to decode lyrics: {}", e))?;
    let lyrics = String::from_utf8(decoded).map_err(|e| anyhow::anyhow!("Failed to convert lyrics to UTF-8: {}", e))?;
    Ok(format_lyrics(title, &lyrics))
}

/// The QQ Music API returns lyrics in the below format:
/// 
/// ```
/// [tag]content\n[tag]content\n[tag]content\n[tag]content\n[tag]content\n
/// ```
/// 
/// This function strips the tag, filters out credit lines (e.g., 
/// "作词：", "作曲：", "演唱：", "编曲：", "吉他：", "混音：", "制作人：",
/// "商用授权：", "【未经著作权人许可不得翻唱翻录或使用】")
/// and returns the first 5 non-empty lyrics lines in a string
fn format_lyrics(title: &str, lyrics: &str) -> String {
    lyrics
        .lines()
        .map(|line| {
            // Remove time tags using regex-like approach
            if let Some(bracket_start) = line.find('[') {
                if let Some(bracket_end) = line.find(']') {
                    if bracket_start < bracket_end {
                        // Check if it's a time tag (contains colon and numbers)
                        return line[bracket_end + 1..].to_string();
                    }
                }
            }
            line.to_string()
        })
        .filter(|line| !line.trim().is_empty()) // Remove empty lines
        .filter(|line| {
            // Filter out credit lines (lines containing "：" followed by names/info)
            let trimmed = line.trim();
            !trimmed.contains("：") &&  // Filter out lines like "作词："
            !trimmed.contains("【") && // Filter out lines like 【未经著作权人许可不得翻唱翻录或使用】
            !trimmed.contains(title) // Filter out lines that contain the title
        })
        .take(5) // Keep only first 5 non-empty lines
        .collect::<Vec<String>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    use wasm_bindgen_test::{console_log, wasm_bindgen_test};

    #[ignore = "Ignore test that needs network acccess"]
    #[wasm_bindgen_test]
    async fn test_qq_music_search_song() {
        let songs = qq_music_search_song(SearchSongQuery { title: "第57次取消发送".to_string() }).await.unwrap();
        console_log!("{:?}", songs);
    }
}
