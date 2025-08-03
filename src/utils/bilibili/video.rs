// https://github.com/SocialSisterYi/bilibili-API-collect/blob/e99f64c9b5c2bbd156e95ca254620378a22697f7/docs/video/info.md

use crate::utils::bilibili::{BiliClient, Response};

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct VideoInfoData {
    pub bvid: String,
    pub title: String,
    pub pubdate: i64,
    pub pages: Vec<VideoPage>,
    pub owner: VideoOwner,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct VideoPage {
    pub cid: i64,
    pub duration: i64,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct VideoOwner {
    pub mid: i64,
}

impl BiliClient {
    pub async fn video_info(&self, bvid: impl AsRef<str>) -> anyhow::Result<VideoInfoData> {
        const URL: &str = "https://api.bilibili.com/x/web-interface/view";
        let req = self.get(URL).query(&[("bvid", bvid.as_ref())]);
        let res: Response<VideoInfoData> = req.send().await?.error_for_status()?.json().await?;

        if res.code != 0 {
            return Err(anyhow::anyhow!("Failed to get video info: code={}, message={}", res.code, res.message.unwrap_or_default()));
        }

        Ok(res.data.expect("No data in response"))
    }
}