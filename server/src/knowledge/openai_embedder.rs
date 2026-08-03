use super::embedder::{validate_embeddings, EmbeddingError, EmbeddingProvider};
use crate::config::EmbeddingConfig;
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

pub struct OpenAiCompatibleEmbeddingProvider {
    client: Client,
    endpoint: String,
    api_key: String,
    model: String,
    dimension: usize,
}

#[derive(Serialize)]
struct EmbeddingRequest<'a> {
    input: &'a [String],
    model: &'a str,
    dimensions: usize,
    encoding_format: &'static str,
}

#[derive(Deserialize)]
struct EmbeddingResponse {
    data: Vec<EmbeddingDatum>,
}

#[derive(Deserialize)]
struct EmbeddingDatum {
    index: usize,
    embedding: Vec<f32>,
}

impl OpenAiCompatibleEmbeddingProvider {
    pub fn new(config: &EmbeddingConfig) -> Result<Self, EmbeddingError> {
        let api_key = config
            .api_key
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| EmbeddingError::Configuration("API key is required".into()))?
            .to_string();
        let client = Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs.max(1)))
            .build()
            .map_err(|error| EmbeddingError::Configuration(error.to_string()))?;
        Ok(Self {
            client,
            endpoint: format!("{}/embeddings", config.base_url.trim_end_matches('/')),
            api_key,
            model: config.model.clone(),
            dimension: config.dimension,
        })
    }
}

#[async_trait]
impl EmbeddingProvider for OpenAiCompatibleEmbeddingProvider {
    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbeddingError> {
        if texts.is_empty() || texts.len() > 256 || texts.iter().any(|text| text.trim().is_empty())
        {
            return Err(EmbeddingError::InvalidOutput(
                "embedding batch must contain 1 to 256 non-empty inputs".into(),
            ));
        }
        let response = self
            .client
            .post(&self.endpoint)
            .bearer_auth(&self.api_key)
            .json(&EmbeddingRequest {
                input: texts,
                model: &self.model,
                dimensions: self.dimension,
                encoding_format: "float",
            })
            .send()
            .await
            .map_err(|error| EmbeddingError::Request(error.to_string()))?;
        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            let bounded = body.chars().take(500).collect::<String>();
            return Err(EmbeddingError::Request(format!(
                "upstream returned {status}: {bounded}"
            )));
        }
        let mut data = response
            .json::<EmbeddingResponse>()
            .await
            .map_err(|error| EmbeddingError::InvalidOutput(error.to_string()))?
            .data;
        data.sort_by_key(|datum| datum.index);
        if data
            .iter()
            .enumerate()
            .any(|(expected, datum)| datum.index != expected)
        {
            return Err(EmbeddingError::InvalidOutput(
                "response indexes are not contiguous".into(),
            ));
        }
        let embeddings = data
            .into_iter()
            .map(|datum| datum.embedding)
            .collect::<Vec<_>>();
        validate_embeddings(&embeddings, texts.len(), self.dimension)?;
        Ok(embeddings)
    }

    fn provider_name(&self) -> &str {
        "openai_compatible"
    }

    fn model_name(&self) -> &str {
        &self.model
    }

    fn dimension(&self) -> usize {
        self.dimension
    }
}
