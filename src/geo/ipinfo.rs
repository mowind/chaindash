use std::time::Duration;

use reqwest::Client;
use serde_json::Value;

use super::snapshot::parse_loc;
use crate::error::{
    ChaindashError,
    Result,
};

const DEFAULT_IPINFO_BASE_URL: &str = "https://ipinfo.io";
const IPINFO_REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// A successfully parsed IPinfo response for a single IP.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IpInfoEntry {
    pub ip: String,
    pub country: Option<String>,
    /// Raw `lat,lng` string, only kept when it parses as valid coordinates.
    pub loc: Option<String>,
}

/// Client for the public IPinfo `/{ip}/json` endpoint, used without a token.
pub struct IpInfoClient {
    http: Client,
    base_url: String,
}

impl IpInfoClient {
    /// Build a client against the public IPinfo endpoint.
    pub fn new() -> Self {
        Self::with_base_url(DEFAULT_IPINFO_BASE_URL.to_string())
    }

    /// Build a client against a custom base URL (used by tests).
    pub(crate) fn with_base_url(base_url: String) -> Self {
        IpInfoClient {
            http: Client::builder()
                .timeout(IPINFO_REQUEST_TIMEOUT)
                .build()
                .expect("http client should build"),
            base_url,
        }
    }

    pub(crate) async fn lookup(
        &self,
        ip: &str,
    ) -> Result<Option<IpInfoEntry>> {
        let url = format!("{}/{ip}/json", self.base_url);
        let response = self.http.get(&url).send().await?;
        if !response.status().is_success() {
            return Err(ChaindashError::Http(format!("ipinfo returned {}", response.status())));
        }
        let body = response.text().await?;
        parse_ipinfo_response(&body)
    }
}

/// Parse an IPinfo `/{ip}/json` response body.
///
/// Returns `Ok(None)` when the response contains no usable data (missing or
/// invalid `country` and `loc`). Malformed JSON is an error.
pub(crate) fn parse_ipinfo_response(body: &str) -> Result<Option<IpInfoEntry>> {
    let value: Value = serde_json::from_str(body)?;

    let country = value
        .get("country")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|country| !country.is_empty())
        .map(str::to_string);

    let loc = value
        .get("loc")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|loc| parse_loc(loc).is_some())
        .map(str::to_string);

    if country.is_none() && loc.is_none() {
        return Ok(None);
    }

    let ip = value.get("ip").and_then(Value::as_str).unwrap_or("").to_string();

    Ok(Some(IpInfoEntry { ip, country, loc }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_ipinfo_response_with_country_and_loc() {
        let entry = parse_ipinfo_response(
            r#"{"ip": "39.99.168.168", "hostname": "x", "city": "Beijing", "country": "CN", "loc": "39.9042,116.4074", "org": "AS1"}"#,
        )
        .unwrap()
        .expect("entry should exist");

        assert_eq!(entry.ip, "39.99.168.168");
        assert_eq!(entry.country.as_deref(), Some("CN"));
        assert_eq!(entry.loc.as_deref(), Some("39.9042,116.4074"));
    }

    #[test]
    fn test_parse_ipinfo_response_with_country_only() {
        let entry = parse_ipinfo_response(r#"{"ip": "1.1.1.1", "country": "US"}"#)
            .unwrap()
            .expect("entry should exist");

        assert_eq!(entry.country.as_deref(), Some("US"));
        assert_eq!(entry.loc, None);
    }

    #[test]
    fn test_parse_ipinfo_response_with_loc_only() {
        let entry = parse_ipinfo_response(r#"{"ip": "1.1.1.1", "loc": "37.751,-97.822"}"#)
            .unwrap()
            .expect("entry should exist");

        assert_eq!(entry.country, None);
        assert_eq!(entry.loc.as_deref(), Some("37.751,-97.822"));
    }

    #[test]
    fn test_parse_ipinfo_response_rejects_missing_country_and_loc() {
        assert_eq!(parse_ipinfo_response(r#"{"ip": "1.1.1.1"}"#).unwrap(), None);
        assert_eq!(parse_ipinfo_response(r#"{"country": ""}"#).unwrap(), None);
    }

    #[test]
    fn test_parse_ipinfo_response_rejects_invalid_loc_but_keeps_country() {
        let entry =
            parse_ipinfo_response(r#"{"ip": "1.1.1.1", "country": "US", "loc": "91.0,0.0"}"#)
                .unwrap()
                .expect("country still counts without a usable loc");

        assert_eq!(entry.country.as_deref(), Some("US"));
        assert_eq!(entry.loc, None);
    }

    #[test]
    fn test_parse_ipinfo_response_rejects_malformed_json() {
        assert!(parse_ipinfo_response("not json").is_err());
    }

    #[tokio::test]
    async fn test_lookup_requests_expected_url_path() {
        use tokio::{
            io::{
                AsyncReadExt,
                AsyncWriteExt,
            },
            net::TcpListener,
        };

        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind should work");
        let addr = listener.local_addr().expect("local addr should exist");
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.expect("accept should work");
            let mut buffer = [0u8; 4096];
            let n = socket.read(&mut buffer).await.expect("read should work");
            let request = String::from_utf8_lossy(&buffer[..n]).to_string();
            let response = "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: \
                            67\r\nconnection: close\r\n\r\n{\"ip\": \"39.99.168.168\", \
                            \"country\": \"CN\", \"loc\": \"39.9042,116.4074\"}";
            socket.write_all(response.as_bytes()).await.expect("write should work");
            request
        });

        let client = IpInfoClient::with_base_url(format!("http://{addr}"));
        let entry = client
            .lookup("39.99.168.168")
            .await
            .expect("lookup should succeed")
            .expect("entry should exist");
        assert_eq!(entry.country.as_deref(), Some("CN"));

        let request = server.await.expect("server should finish");
        assert!(
            request.starts_with("GET /39.99.168.168/json HTTP/1.1"),
            "unexpected request: {request}"
        );
    }
}
