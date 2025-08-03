use axum::extract::{Json, State};
use axum_cloudflare_adapter::wasm_compat;
use jiff::{tz::TimeZone, Timestamp};
use serde::Deserialize;

use crate::{api::v1::ApiV1Response, utils::bilibili::Credential, AppState};

#[derive(Debug, Deserialize)]
pub struct DownloadRequest {
    bvid: String,
    /// Authorization token. This API is admin-only.
    token: String,
}

#[derive(Debug, serde::Serialize)]
pub struct DownloadResponse {
    /// Name of the cloudflare r2 bucket where the video is stored
    bucket: String,
    /// A list of downloaded audio files from the given video
    audios: Vec<Audio>,
}

#[derive(Debug, serde::Serialize)]
pub struct Audio {
    /// key of the audio file in the cloudflare r2 bucket
    object_key: String,
    /// upload id of the audio file, for debugging
    /// If the upload is already present, this will be empty.
    upload_id: String,
}

/// Download a bilibili video content and upload to cloudflare r2
#[wasm_compat]
pub async fn download(
    State(state): State<AppState>,
    Json(request): Json<DownloadRequest>,
) -> ApiV1Response {
    use http::StatusCode;
    use crate::utils::bilibili::{BiliClient, video_stream_url::FeatureValue};

    let DownloadRequest { bvid, token } = request;

    if token != state.env.env.var("FIREFLY_API_AUTHN_TOKEN").unwrap().to_string() {
        return ApiV1Response::Error {
            status: StatusCode::UNAUTHORIZED,
            message: "Invalid token".to_string(),
        };
    }

    let credential = match Credential::from_env(&state.env.env) {
        Ok(credential) => credential,
        Err(e) => {
            tracing::error!(error = %e, "Failed to get credential");
            return ApiV1Response::Error {
                status: StatusCode::INTERNAL_SERVER_ERROR,
                message: "Failed to get credential".to_string(),
            };
        }
    };

    let client = BiliClient::new(credential);

    let wbi_keys = match client.get_wbi_keys().await {
        Ok(wbi_keys) => wbi_keys,
        Err(e) => {
            tracing::error!(error = %e, "Failed to get wbi keys");
            return ApiV1Response::Error {
                status: StatusCode::INTERNAL_SERVER_ERROR,
                message: "Failed to get wbi keys".to_string(),
            };
        }
    };

    let info = match client.video_info(bvid.clone()).await {
        Ok(info) => info,
        Err(e) => {
            tracing::error!(error = %e, "Failed to get video info");
            return ApiV1Response::Error {
                status: StatusCode::INTERNAL_SERVER_ERROR,
                message: "Failed to get video info".to_string(),
            };
        }
    };

    tracing::info!(
        bvid = bvid.as_str(),
        title = info.title.as_str(),
        owner = info.owner.mid,
        pubdate = info.pubdate,
        pages = info.pages.len(),
        "Start processing bvid {}", bvid
    );

    let pubdate = info.pubdate;
    let ts = Timestamp::new(pubdate, 0).unwrap();
    let tz = TimeZone::get("Asia/Shanghai").unwrap();
    let zoned = ts.to_zoned(tz);
    let year = zoned.year();
    let month = zoned.month();
    let day = zoned.day();

    let bucket = state.env.env.bucket("firefly_bilibili").expect("Failed to get bucket");

    let mut audios = Vec::new();
    for (index, page) in info.pages.iter().enumerate() {
        // Pages are 1-indexed
        let index = index + 1;

        let stream_url = match client.stream_url(wbi_keys.clone(), bvid.clone(), page.cid, FeatureValue::all_dash()).await {
            Ok(stream_url) => stream_url,
            Err(e) => {
                tracing::error!(error = ?e, "Failed to get stream url");
                return ApiV1Response::Error {
                    status: StatusCode::INTERNAL_SERVER_ERROR,
                    message: "Failed to get stream url".to_string(),
                };
            }
        };

        let Some(selected_audio) = stream_url.dash.audio.first() else { 
            tracing::error!(page = ?page, "No audio found for page");
            return ApiV1Response::Error {
                status: StatusCode::INTERNAL_SERVER_ERROR,
                message: "No audio found for page".to_string(),
            };
        };

        let audio_base_url = selected_audio.base_url.clone();
        // Get the audio file size
        let audio_response = client.get(audio_base_url.clone()).header("Referer", "https://www.bilibili.com/").send().await.unwrap().error_for_status().unwrap();
        let audio_size = audio_response.headers().get("Content-Length").unwrap().to_str().unwrap().parse::<u64>().unwrap();

        // Key: audio/{mid}/{year}/{month}/{day}/{bvid}/{page}.mp4
        let upload_key = format!("audio/{}/{}/{}/{}/{}/{}.mp4", info.owner.mid, year, month, day, bvid, index);

        if let Ok(Some(object)) = bucket.get(&upload_key).execute().await {
            tracing::info!(
                bvid = bvid.as_str(),
                page = index,
                cid = page.cid,
                "Audio already exists for BVID {}, page {}, cid {}", bvid, index, page.cid
            );

            audios.push(Audio {
                object_key: object.key(),
                upload_id: "".to_string(),
            });

            continue;
        }

        tracing::info!(
            bvid = bvid.as_str(),
            page = index,
            cid = page.cid,
            bandwidth = selected_audio.bandwidth,
            codecs = selected_audio.codecs,
            mime_type = selected_audio.mime_type,
            base_url = audio_base_url.as_str(),
            audio_size = audio_size,
            upload_key = upload_key.as_str(),
            "Start downloading audio for BVID {}, page {}, cid {}", bvid, index, page.cid
        );

        // Chunk the audio file into 20MB chunks
        let multipart_upload = bucket.create_multipart_upload(upload_key.clone()).execute().await.expect("Failed to create multipart upload");
        let upload_id = multipart_upload.upload_id().await;

        let mut upload_futures = Vec::new();
        for (idx, (start, end)) in (0..audio_size).step_by(20 * 1024 * 1024).map(|start| (start, (start + 20 * 1024 * 1024).min(audio_size - 1))).enumerate() {
            // Cloudflare R2 multipart upload is 1-indexed
            let idx = idx + 1;

            // let cookie = client.cookie().to_string();
            let audio_base_url = audio_base_url.clone();
            let bucket = bucket.clone();
            let upload_id = upload_id.clone();
            let upload_key = upload_key.clone();
            let cookie = client.cookie().to_string();

            let upload_future = async move {
                let length = end - start + 1;

                // let response = match client.get(audio_base_url.clone()).header("Referer", "https://www.bilibili.com/").header("Range", &format!("bytes={}-{}", start, end)).send().await.expect("Failed to send request").error_for_status() {
                //     Ok(response) => response,
                //     Err(e) => {
                //         tracing::error!(error = %e, "Failed to get audio chunk {}-{}", start, end - 1);
                //         return Err(anyhow::anyhow!("Failed to get audio chunk {}-{}: {:?}", start, end - 1, e));
                //     }
                // };
                // let stream = response.bytes_stream();

                // Not sure why reqwest stream doesn't work. Got either Network Connection Lost or Failed to decode response body.
                // So use worker::Fetch instead.
                let mut request = worker::Request::new(&audio_base_url, worker::Method::Get).unwrap();
                request.headers_mut().unwrap().set("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36").unwrap();
                request.headers_mut().unwrap().set("Cookie", &cookie).unwrap();
                request.headers_mut().unwrap().set("Referer", "https://www.bilibili.com/").unwrap();
                request.headers_mut().unwrap().set("Range", &format!("bytes={}-{}", start, end)).unwrap();

                let mut response = worker::Fetch::Request(request).send().await.unwrap();
                if response.status_code() != 206 {
                    let body = response.text().await.unwrap();
                    tracing::error!(
                        status = response.status_code(),
                        body = body.as_str(),
                        "Failed to get audio chunk {}-{}", start, end - 1
                    );
                    return Err(anyhow::anyhow!("Failed to get audio chunk {}-{}: {:?}", start, end - 1, response.status_code()));
                }

                let stream = response.stream().unwrap();
                let fstream = worker::FixedLengthStream::wrap(stream, length);

                let multipart = bucket.resume_multipart_upload(upload_key, upload_id).expect("Failed to resume multipart upload");
                let part = multipart.upload_part(idx as u16, fstream).await.expect("Failed to upload part");

                tracing::info!(
                    part_number = part.part_number(),
                    part_size = length,
                    etag = part.etag().as_str(),
                    start = start,
                    end = end,
                    "Uploaded audio chunk {}-{}", start, end - 1
                );

                Ok(part)
            };

            upload_futures.push(upload_future);
        }

        let results = futures::future::join_all(upload_futures).await;
        if let Some(Err(_)) = results.iter().find(|result| result.is_err()) {
            multipart_upload.abort().await.expect("Failed to abort multipart upload");

            return ApiV1Response::Error {
                status: StatusCode::INTERNAL_SERVER_ERROR,
                message: "Failed to upload audio chunk".to_string(),
            };
        }

        let object = multipart_upload.complete(results.into_iter().map(|result| result.unwrap())).await.expect("Failed to complete multipart upload");

        tracing::info!(
            object_key = object.key().as_str(),
            size = object.size(),
            etag = object.etag().as_str(),
            upload_date = %object.uploaded(),
            checksum = ?object.checksum(),
            "Uploaded audio for BVID {}, page {}, cid {}", bvid, index, page.cid
        );

        audios.push(Audio {
            object_key: upload_key,
            upload_id,
        });
    }

    let response = DownloadResponse {
        bucket: "firefly-bilibili".to_string(),
        audios,
    };

    let s = serde_json::to_string(&response).unwrap();
    ApiV1Response::Ok(s)
}

#[cfg(test)]
mod tests {
    use wasm_bindgen_test::{console_log, wasm_bindgen_test};

    use crate::utils::bilibili::{must_test_client, video_stream_url::FeatureValue};

    #[ignore = "Ignore test that needs network acccess"]
    #[wasm_bindgen_test]
    async fn test_get_audio_streaming_url() {
        let client = must_test_client();
        let wbi_keys = client.get_wbi_keys().await.unwrap();
        let info = client.video_info("BV1kemrYmEAJ").await.unwrap();
        for page in info.pages {
            let stream_url = client
                .stream_url(
                    wbi_keys.clone(),
                    "BV1kemrYmEAJ",
                    page.cid,
                    FeatureValue::all_dash(),
                )
                .await
                .unwrap();
            console_log!("{:?}", stream_url);
            console_log!(
                "Download audio from {:?}",
                stream_url.dash.audio[0].base_url
            );
            let audio_url = stream_url.dash.audio[0].base_url.clone();
            let audio_response = client.get(audio_url).header("Referer", "https://www.bilibili.com/").send().await.unwrap().error_for_status().unwrap();
            console_log!("{:?}", audio_response.headers().get("Content-Length"));
        }
    }
}
