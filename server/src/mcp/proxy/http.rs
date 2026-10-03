//! Remote MCP servers over streamable HTTP.

use super::client::{self, Redactor};
use super::transport::{McpConnection, McpTransport, ProxyError};
use crate::plugins::placeholders::ResolvedTransport;
use async_trait::async_trait;
use reqwest::header::{HeaderName, HeaderValue};
use reqwest::Url;
use rmcp::model::ClientJsonRpcMessage;
use rmcp::transport::common::client_side_sse::BoxedSseResponse;
use rmcp::transport::streamable_http_client::{
    StreamableHttpClient, StreamableHttpClientTransportConfig, StreamableHttpError,
    StreamableHttpPostResponse,
};
use rmcp::transport::StreamableHttpClientTransport;
use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::sync::Arc;

type HttpResult<T> = Result<T, StreamableHttpError<reqwest_mcp::Error>>;

/// rmcp's reqwest client with the URL stripped from every error: rmcp logs these
/// errors itself, and reqwest's `Display` appends the full URL.
#[derive(Clone)]
struct UrlFreeClient(reqwest_mcp::Client);

impl UrlFreeClient {
    /// Same settings as rmcp's default client: no idle pooling, no redirects.
    fn new() -> Result<Self, ProxyError> {
        reqwest_mcp::Client::builder()
            .pool_max_idle_per_host(0)
            .redirect(reqwest_mcp::redirect::Policy::none())
            .build()
            .map(Self)
            .map_err(|e| ProxyError::Start(format!("http client: {}", e.without_url())))
    }
}

fn without_url<T>(result: HttpResult<T>) -> HttpResult<T> {
    result.map_err(|err| match err {
        StreamableHttpError::Client(e) => StreamableHttpError::Client(e.without_url()),
        other => other,
    })
}

impl StreamableHttpClient for UrlFreeClient {
    type Error = reqwest_mcp::Error;

    async fn post_message(
        &self,
        uri: Arc<str>,
        message: ClientJsonRpcMessage,
        session_id: Option<Arc<str>>,
        auth_header: Option<String>,
        custom_headers: HashMap<HeaderName, HeaderValue>,
    ) -> HttpResult<StreamableHttpPostResponse> {
        without_url(
            self.0
                .post_message(uri, message, session_id, auth_header, custom_headers)
                .await,
        )
    }

    async fn post_message_with_max_sse_event_size(
        &self,
        uri: Arc<str>,
        message: ClientJsonRpcMessage,
        session_id: Option<Arc<str>>,
        auth_header: Option<String>,
        custom_headers: HashMap<HeaderName, HeaderValue>,
        max_sse_event_size: usize,
    ) -> HttpResult<StreamableHttpPostResponse> {
        without_url(
            self.0
                .post_message_with_max_sse_event_size(
                    uri,
                    message,
                    session_id,
                    auth_header,
                    custom_headers,
                    max_sse_event_size,
                )
                .await,
        )
    }

    async fn delete_session(
        &self,
        uri: Arc<str>,
        session_id: Arc<str>,
        auth_header: Option<String>,
        custom_headers: HashMap<HeaderName, HeaderValue>,
    ) -> HttpResult<()> {
        without_url(
            self.0
                .delete_session(uri, session_id, auth_header, custom_headers)
                .await,
        )
    }

    async fn get_stream(
        &self,
        uri: Arc<str>,
        session_id: Option<Arc<str>>,
        last_event_id: Option<String>,
        auth_header: Option<String>,
        custom_headers: HashMap<HeaderName, HeaderValue>,
    ) -> HttpResult<BoxedSseResponse> {
        without_url(
            self.0
                .get_stream(uri, session_id, last_event_id, auth_header, custom_headers)
                .await,
        )
    }

    async fn get_stream_with_max_sse_event_size(
        &self,
        uri: Arc<str>,
        session_id: Option<Arc<str>>,
        last_event_id: Option<String>,
        auth_header: Option<String>,
        custom_headers: HashMap<HeaderName, HeaderValue>,
        max_sse_event_size: usize,
    ) -> HttpResult<BoxedSseResponse> {
        without_url(
            self.0
                .get_stream_with_max_sse_event_size(
                    uri,
                    session_id,
                    last_event_id,
                    auth_header,
                    custom_headers,
                    max_sse_event_size,
                )
                .await,
        )
    }
}

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
        let ResolvedTransport::Http {
            url,
            headers,
            secrets,
        } = spec
        else {
            return Err(ProxyError::Start(format!(
                "http transport cannot open a {} server",
                spec.kind()
            )));
        };
        let redactor = redactor(url, headers, secrets);
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
        let transport = StreamableHttpClientTransport::with_client(UrlFreeClient::new()?, config);
        client::connect(transport, redactor, Redactor::for_output(secrets)).await
    }
}

const SHOWN_URL: &str = "<server url>";

/// Shorter host labels and path segments are too common to redact safely.
const MIN_URL_PART_LEN: usize = 4;

/// Any part of the URL may come from a setting, so none of it is shown. `secrets`
/// go last so URL parts keep the `<server url>` replacement.
fn redactor(url: &str, headers: &BTreeMap<String, String>, secrets: &[String]) -> Redactor {
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
    secrets.iter().fold(redactor, |r, value| r.secret(value))
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
        let redactor = redactor(url, &headers, &[]);
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
        let redactor = redactor(url, &BTreeMap::new(), &[]);
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

    #[test]
    fn bare_secret_from_bearer_header_is_redacted() {
        use crate::plugins::capability::McpServerTransport;
        use crate::plugins::placeholders::{resolve, ResolveCtx};

        let transport = McpServerTransport::Http {
            url: "https://example.com/mcp".into(),
            headers: BTreeMap::from([(
                "Authorization".to_string(),
                "Bearer ${API_TOKEN}".to_string(),
            )]),
        };
        let settings = BTreeMap::from([("API_TOKEN".to_string(), "tk-1".to_string())]);
        let ctx = ResolveCtx {
            plugin_root: Path::new("/p"),
            settings: &settings,
            env: &|_| None,
        };
        let ResolvedTransport::Http {
            url,
            headers,
            secrets,
        } = resolve(&transport, &ctx).unwrap()
        else {
            panic!("expected http");
        };
        let message = redactor(&url, &headers, &secrets).apply("token tk-1 rejected");
        assert_eq!(message, "token [redacted] rejected");
    }
}
