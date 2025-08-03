//! https://github.com/SocialSisterYi/bilibili-API-collect/blob/e99f64c9b5c2bbd156e95ca254620378a22697f7/docs/video/videostream_url.md

use bitflags::bitflags;

use crate::utils::bilibili::{wbi::encode_wbi, BiliClient, Response};

#[derive(Debug, serde_repr::Deserialize_repr, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
#[repr(u32)]
pub enum QualityCode {
    P240 = 6,
    P360 = 16,
    P480 = 32,
    P720 = 64,
    P720HFR = 74,
    P1080 = 80,
    AiAugmented = 100,
    P1080HBR = 112,
    P1080HFR = 116,
    P4K = 120,
    Hdr = 125,
    DolbyVision = 126,
    P8K = 127,
    #[serde(other)]
    Unknown = 0,
}

bitflags! {
    #[derive(Debug, serde::Deserialize, Default, Clone, Copy)]
    pub struct FeatureValue: u32 {
        const FLV = 0; // Deprecated
        const MP4 = 1;
        const DASH = 1 << 4; // 1 << 4 = 16
        const HDR = 1 << 6; // 1 << 6 = 64
        const P4k = 1 << 7; // 1 << 7 = 128
        const DolbyAudio = 1 << 8; // 1 << 8 = 256
        const DolbyVision = 1 << 9; // 1 << 9 = 512
        const P8k = 1 << 10; // 1 << 10 = 1024
        const AV1 = 1 << 11; // 1 << 11 = 2048
    }
}

impl FeatureValue {
    pub fn all_dash() -> Self {
        // 4048
        Self::DASH | Self::DolbyAudio | Self::DolbyVision | Self::P4k | Self::P8k | Self::AV1
    }
}

#[cfg(test)]
#[test]
fn test_all_dash() {
    assert_eq!(FeatureValue::all_dash().bits(), 4048);
}

#[derive(Debug, serde::Deserialize)]
pub struct VideostreamUrlData {
    // pub quality: QualityCode,
    pub dash: Dash,
}

#[derive(Debug, serde::Deserialize)]
pub struct Dash {
    #[serde(default)]
    pub audio: Vec<DashAudio>,
}

#[derive(Debug, serde::Deserialize)]
pub struct DashAudio {
    // pub id: i64,
    pub base_url: String,
    pub bandwidth: u32,
    pub codecs: String,
    pub mime_type: String,
}

impl BiliClient {
    pub async fn stream_url(
        &self,
        wbi_keys: (String, String),
        bvid: impl AsRef<str>,
        cid: i64,
        fnval: FeatureValue,
    ) -> anyhow::Result<VideostreamUrlData> {
        const URL: &str = "https://api.bilibili.com/x/player/wbi/playurl";

        let params = vec![
            ("bvid", bvid.as_ref().to_string()),
            ("cid", cid.to_string()),
            ("fnval", fnval.bits().to_string()),
        ];

        let wbi_encoded_params = encode_wbi(params, wbi_keys);

        let url = format!("{}?{}", URL, wbi_encoded_params);
        let req = self.get(url);

        let res: Response<VideostreamUrlData> = req.send().await?.error_for_status()?.json().await?;

        if res.code != 0 {
            return Err(anyhow::anyhow!("Failed to get stream url: code={}, message={}", res.code, res.message.unwrap_or_default()));
        }

        Ok(res.data.expect("No data in response"))
    }
}
