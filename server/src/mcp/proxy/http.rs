//! Remote MCP servers over streamable HTTP.

use super::client::{self, Redactor};
use super::transport::{McpConnection, McpTransport, ProxyError};
use crate::plugins::placeholders::ResolvedTransport;
use async_trait::async_trait;
use reqwest::header::{HeaderName, HeaderValue};
use reqwest::Url;
use rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig;
use rmcp::transport::StreamableHttpClientTransport;
use std::collections::{BTreeMap, HashMap};
use std::path::Path;

pub struct HttpTransport;

#[async_trait]
impl McpTransport for HttpTransport {
    fn kind(&self) -> &'static str {
        "http"
    }

    async fn connect(
        &self,
        spec: &ResolvedTransport,
        _cwd: &Path,
    ) -> Result<Box<dyn McpConnection>, ProxyError> {
        let ResolvedTransport::Http { url, headers } = spec else {
            return Err(ProxyError::Start(format!(
                "http transport cannot open a {} server",
                spec.kind()
            )));
        };
        let redactor = redactor(url, headers);
        let parsed = Url::parse(url).map_err(|_| ProxyError::Start("invalid url".into()))?;
        if !matches!(parsed.scheme(), "http" | "https") {
            return Err(ProxyError::Start(format!(
                "unsupported url scheme \"{}\"",
                parsed.scheme()
            )));
        }
        let mut custom = HashMap::new();
        for (name, value) in headers {
            let header = HeaderName::from_bytes(name.as_bytes())
                .map_err(|_| ProxyError::Start(format!("invalid header name \"{name}\"")))?;
            let mut value = HeaderValue::from_str(value)
                .map_err(|_| ProxyError::Start(format!("invalid value for header \"{name}\"")))?;
            value.set_sensitive(true);
            custom.insert(header, value);
        }
        let config =
            StreamableHttpClientTransportConfig::with_uri(url.as_str()).custom_headers(custom);
        client::connect(StreamableHttpClientTransport::from_config(config), redactor).await
    }
}

const SHOWN_URL: &str = "<server url>";

/// Shorter host labels and path segments are too common to redact safely.
const MIN_URL_PART_LEN: usize = 4;

/// Any part of the URL may come from a setting, so none of it is shown.
fn redactor(url: &str, headers: &BTreeMap<String, String>) -> Redactor {
    let mut redactor = headers
        .values()
        .fold(Redactor::default(), |r, value| r.secret(value))
        .replace(url, SHOWN_URL);
    match Url::parse(url) {
        Ok(parsed) => {
            let host = parsed.host_str().unwrap_or_default();
            let authority = match parsed.port() {
                Some(port) => format!("{host}:{port}"),
                None => host.to_string(),
            };
            let origin = format!("{}://{authority}", parsed.scheme());
            let host_path = format!("{authority}{}", parsed.path());
            redactor = redactor
                .replace(parsed.as_str(), SHOWN_URL)
                .replace(&format!("{origin}{}", parsed.path()), SHOWN_URL)
                .replace(&origin, SHOWN_URL)
                .replace(&host_path, SHOWN_URL)
                .replace(&authority, SHOWN_URL);
            let normalized = parsed.path_segments().into_iter().flatten();
            for part in normalized.chain([host]).chain(raw_url_parts(url)) {
                if part.len() >= MIN_URL_PART_LEN {
                    redactor = redactor.replace(part, SHOWN_URL);
                }
            }
            redactor = redactor
                .secret(parsed.username())
                .secret(parsed.password().unwrap_or_default())
                .secret(parsed.query().unwrap_or_default())
                .secret(parsed.fragment().unwrap_or_default());
        }
        Err(_) => redactor = redactor.secret(url),
    }
    redactor
}

/// Host and path segments as written, before URL normalization re-encodes them.
fn raw_url_parts(url: &str) -> impl Iterator<Item = &str> {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let rest = rest.split(['?', '#']).next().unwrap_or_default();
    let rest = rest.rsplit_once('@').map_or(rest, |(_, host)| host);
    rest.split('/').flat_map(|part| {
        let host = part.rsplit_once(':').map(|(host, _)| host);
        [Some(part), host].into_iter().flatten()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_credentials_and_headers() {
        let url = "https://alice:pw-1@example.com:8443/mcp/v1?key=q-1#frag-1";
        let headers = BTreeMap::from([("Authorization".to_string(), "Bearer h-1".to_string())]);
        let redactor = redactor(url, &headers);
        let message = redactor.apply(&format!(
            "error sending request for url ({url}): auth Bearer h-1 rejected for alice"
        ));
        assert_eq!(
            message,
            "error sending request for url (<server url>): \
             auth [redacted] rejected for [redacted]"
        );
    }

    #[test]
    fn hides_every_part_of_the_url() {
        let url = "https://hooks.example.com:8443/hooks/p4th s3cr3t/mcp";
        let redactor = redactor(url, &BTreeMap::new());
        for leaked in [
            url,
            "https://hooks.example.com:8443/hooks/p4th%20s3cr3t/mcp",
            "hooks.example.com:8443/hooks/p4th%20s3cr3t/mcp",
            "https://hooks.example.com:8443",
            "connect to hooks.example.com failed",
            "segment p4th s3cr3t",
            "segment p4th%20s3cr3t",
            "segment hooks",
        ] {
            let message = redactor.apply(leaked);
            for part in ["hooks", "example.com", "p4th", "s3cr3t", "8443"] {
                assert!(!message.contains(part), "{part} leaked in {message}");
            }
        }
    }
}
