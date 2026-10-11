use std::time::Duration;

use reqwest::{Client, RequestBuilder, Response};
use serde::de::DeserializeOwned;

use crate::error::{Error, Result};

pub type HttpClient = Client;

pub const USER_AGENT: &str = concat!("SlattyLauncher/", env!("CARGO_PKG_VERSION"));

/// A connection silent for 30 seconds is given up; a slow one never is, whatever it carries: a
/// file of a Linux installer comes in a single request, and may take many minutes.
pub fn client() -> Result<Client> {
    Client::builder()
        .user_agent(USER_AGENT)
        .connect_timeout(Duration::from_secs(10))
        .read_timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| Error::network("building the HTTP client", e))
}

/// `url`, when it is one of GOG's own addresses over HTTPS: the only kind an address read from one
/// of GOG's answers may be when the account's token goes with it.
pub fn gog_only(url: &str) -> Result<&str> {
    let gog = url::Url::parse(url).is_ok_and(|u| {
        u.scheme() == "https"
            && u.host_str()
                .is_some_and(|h| h == "gog.com" || h.ends_with(".gog.com"))
    });
    if gog {
        Ok(url)
    } else {
        Err(Error::Refused(
            "an address outside GOG was not sent the account's token".into(),
        ))
    }
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

/// The JSON body of a request that only reads, sent again like `bytes` when it failed on the way.
pub async fn json<T: DeserializeOwned>(req: RequestBuilder, context: &'static str) -> Result<T> {
    let raw = bytes(req, context).await?;
    serde_json::from_slice(&raw).map_err(|e| Error::parse(context, e))
}

/// The body of a request that only reads. A request that failed on the way (dropped connection,
/// timeout, rate limit, server error) is sent again, after one second then two.
pub async fn bytes(req: RequestBuilder, context: &'static str) -> Result<Vec<u8>> {
    let mut wait = FIRST_WAIT;
    for _ in 1..ATTEMPTS {
        let Some(attempt) = req.try_clone() else {
            break;
        };
        match bytes_once(attempt, context).await {
            Err(e) if e.is_temporary() => {
                tokio::time::sleep(wait).await;
                wait *= 2;
            }
            done => return done,
        }
    }
    bytes_once(req, context).await
}

const ATTEMPTS: u32 = 3;
const FIRST_WAIT: Duration = if cfg!(test) {
    Duration::from_millis(10)
} else {
    Duration::from_secs(1)
};

async fn bytes_once(req: RequestBuilder, context: &'static str) -> Result<Vec<u8>> {
    let resp = send(req, context).await?;
    let body = resp.bytes().await.map_err(|e| Error::network(context, e))?;
    Ok(body.to_vec())
}

#[cfg(test)]
mod tests {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    use super::*;

    /// Serves one canned response per connection, in order.
    async fn serve(responses: Vec<&'static str>) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            for body in responses {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut buf = [0u8; 4096];
                let _ = socket.read(&mut buf).await;
                let _ = socket.write_all(body.as_bytes()).await;
            }
        });
        format!("http://{addr}/")
    }

    const UNAVAILABLE: &str =
        "HTTP/1.1 503 Service Unavailable\r\ncontent-length: 0\r\nconnection: close\r\n\r\n";
    const NOT_FOUND: &str =
        "HTTP/1.1 404 Not Found\r\ncontent-length: 0\r\nconnection: close\r\n\r\n";
    const OK: &str = "HTTP/1.1 200 OK\r\ncontent-length: 7\r\ncontent-type: application/json\r\nconnection: close\r\n\r\n{\"a\":1}";

    #[tokio::test]
    async fn a_server_error_is_tried_again() {
        let url = serve(vec![UNAVAILABLE, UNAVAILABLE, OK]).await;
        let v: serde_json::Value = json(client().unwrap().get(url), "testing").await.unwrap();
        assert_eq!(v["a"], 1);
    }

    #[tokio::test]
    async fn an_answer_is_not_asked_again() {
        let url = serve(vec![NOT_FOUND, OK]).await;
        let r = json::<serde_json::Value>(client().unwrap().get(url), "testing").await;
        assert!(matches!(r, Err(Error::Http { status: 404, .. })));
    }

    /// A large file (a Linux installer's data, a big save) keeps arriving for minutes on a slow
    /// connection: only a silent connection is given up, never a long one. Takes over two minutes.
    #[tokio::test]
    #[ignore]
    async fn a_long_download_is_not_cut_off() {
        const SECONDS: usize = 125;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buf = [0u8; 4096];
            let _ = socket.read(&mut buf).await;
            let head = format!("HTTP/1.1 200 OK\r\ncontent-length: {SECONDS}\r\n\r\n");
            socket.write_all(head.as_bytes()).await.unwrap();
            for _ in 0..SECONDS {
                socket.write_all(b"x").await.unwrap();
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        });
        let resp = send(client().unwrap().get(format!("http://{addr}/")), "testing")
            .await
            .unwrap();
        assert_eq!(resp.bytes().await.unwrap().len(), SECONDS);
    }

    #[test]
    fn only_gogs_own_addresses_are_sent_the_token() {
        for ok in [
            "https://api.gog.com/products/1/downlink/installer/en1installer0",
            "https://gog-cdn-fastly.gog.com/content-system/v2/patches/meta/1",
            "https://gog.com/x",
        ] {
            assert!(gog_only(ok).is_ok(), "{ok}");
        }
        for refused in [
            "http://api.gog.com/x",
            "https://evil.example/x",
            "https://api.gog.com.evil.example/x",
            "https://evilgog.com/x",
            "not an address",
        ] {
            assert!(
                matches!(gog_only(refused), Err(Error::Refused(_))),
                "{refused}"
            );
        }
    }

    #[tokio::test]
    async fn retries_stop_after_three_attempts() {
        let url = serve(vec![UNAVAILABLE, UNAVAILABLE, UNAVAILABLE, OK]).await;
        let r = json::<serde_json::Value>(client().unwrap().get(url), "testing").await;
        assert!(matches!(r, Err(Error::Http { status: 503, .. })));
    }
}
