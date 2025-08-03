use std::ops::Deref;

use worker::Env;

pub mod video;
pub mod video_stream_url;
mod wbi;

#[derive(Debug, Clone)]
pub struct Credential {
    cookie: String,
}

impl Credential {
    #[cfg(test)]
    pub fn new_for_test(sessdata: impl AsRef<str>) -> Self {
        let cookie = format!("SESSDATA={}", sessdata.as_ref());
        Self { cookie }
    }

    pub fn from_env(env: &Env) -> anyhow::Result<Self> {
        let sessdata = env.var("BILIBILI_SESSDATA")?.to_string();
        let cookie = format!("SESSDATA={}", sessdata);
        Ok(Self { cookie })
    }

    pub fn cookie(&self) -> &str {
        &self.cookie
    }
}

#[derive(Debug, serde::Deserialize)]
pub struct Response<T> {
    pub code: i32,
    pub message: Option<String>,
    pub data: Option<T>,
}

#[derive(Debug, Clone)]
pub struct BiliClient {
    credential: Credential,
    inner: reqwest::Client,
}

impl BiliClient {
    pub fn new(credential: Credential) -> Self {
        let mut default_headers = reqwest::header::HeaderMap::new();
        default_headers.insert("Cookie", credential.cookie().parse().unwrap());
        default_headers.insert("User-Agent", "Mozilla/5.0 (Windows NT 10.0; WOW64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/119.0.0.0 Safari/537.36".parse().unwrap());

        let inner = reqwest::Client::builder()
            .default_headers(default_headers)
            .build()
            .unwrap();

        Self {
            credential,
            inner,
        }
    }

    pub fn cookie(&self) -> &str {
        self.credential.cookie()
    }
}

impl Deref for BiliClient {
    type Target = reqwest::Client;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

#[cfg(test)]
pub fn must_test_client() -> BiliClient {
    let env = crate::utils::env();
    let sessdata = env.get("BILIBILI_SESSDATA").expect("BILIBILI_SESSDATA is not set");
    BiliClient::new(Credential::new_for_test(sessdata))
}