use std::time::Duration;

use reqwest::{Client, RequestBuilder, Response};
use serde::de::DeserializeOwned;

use crate::error::{Error, Result};

pub type HttpClient = Client;

pub const USER_AGENT: &str = concat!("SlattyLauncher/", env!("CARGO_PKG_VERSION"));

pub fn client() -> Result<Client> {
    Client::builder()
        .user_agent(USER_AGENT)
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|e| Error::network("building the HTTP client", e))
}

pub async fn send(req: RequestBuilder, context: &'static str) -> Result<Response> {
    let resp = req.send().await.map_err(|e| Error::network(context, e))?;
    let status = resp.status();
    if status.is_success() {
        Ok(resp)
    } else {
        Err(Error::Http {
            context,
            status: status.as_u16(),
        })
    }
}

pub async fn json<T: DeserializeOwned>(req: RequestBuilder, context: &'static str) -> Result<T> {
    let resp = send(req, context).await?;
    let bytes = resp.bytes().await.map_err(|e| Error::network(context, e))?;
    serde_json::from_slice(&bytes).map_err(|e| Error::parse(context, e))
}
