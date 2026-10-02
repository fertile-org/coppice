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

/// Scheme, host, port, and path only — no userinfo, query, or fragment.
fn display_url(url: &Url) -> String {
    let mut shown = format!("{}://{}", url.scheme(), url.host_str().unwrap_or_default());
    if let Some(port) = url.port() {
        shown.push_str(&format!(":{port}"));
    }
    shown.push_str(url.path());
    shown
}

fn redactor(url: &str, headers: &BTreeMap<String, String>) -> Redactor {
    let mut redactor = headers
        .values()
        .fold(Redactor::default(), |r, value| r.secret(value));
    match Url::parse(url) {
        Ok(parsed) => {
            let shown = display_url(&parsed);
            redactor = redactor
                .replace(url, &shown)
                .replace(parsed.as_str(), &shown)
                .secret(parsed.username())
                .secret(parsed.password().unwrap_or_default())
                .secret(parsed.query().unwrap_or_default())
                .secret(parsed.fragment().unwrap_or_default());
        }
        Err(_) => redactor = redactor.secret(url),
    }
    redactor
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
            "error sending request for url (https://example.com:8443/mcp/v1): \
             auth [redacted] rejected for [redacted]"
        );
    }
}
