use super::embedder::{validate_embeddings, EmbeddingError, EmbeddingProvider};
use async_trait::async_trait;
use sha2::{Digest, Sha256};

pub struct MockEmbeddingProvider {
    dimension: usize,
    model: String,
}

impl MockEmbeddingProvider {
    pub fn new(dimension: usize, model: String) -> Self {
        Self { dimension, model }
    }

    fn embed_one(&self, text: &str) -> Vec<f32> {
        let normalized = text.trim().replace("\r\n", "\n");
        let mut values = Vec::with_capacity(self.dimension);
        let mut block = 0u64;
        while values.len() < self.dimension {
            let mut hasher = Sha256::new();
            hasher.update(normalized.as_bytes());
            hasher.update(block.to_le_bytes());
            let digest = hasher.finalize();
            for chunk in digest.chunks_exact(2) {
                let raw = u16::from_le_bytes([chunk[0], chunk[1]]);
                values.push((raw as f32 / u16::MAX as f32) * 2.0 - 1.0);
                if values.len() == self.dimension {
                    break;
                }
            }
            block += 1;
        }
        let norm = values.iter().map(|value| value * value).sum::<f32>().sqrt();
        if norm > 0.0 {
            for value in &mut values {
                *value /= norm;
            }
        }
        values
    }
}

#[async_trait]
impl EmbeddingProvider for MockEmbeddingProvider {
    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbeddingError> {
        if texts.is_empty() || texts.iter().any(|text| text.trim().is_empty()) {
            return Err(EmbeddingError::InvalidOutput(
                "embedding input must not be empty".into(),
            ));
        }
        let embeddings = texts
            .iter()
            .map(|text| self.embed_one(text))
            .collect::<Vec<_>>();
        validate_embeddings(&embeddings, texts.len(), self.dimension)?;
        Ok(embeddings)
    }

    fn provider_name(&self) -> &str {
        "mock"
    }

    fn model_name(&self) -> &str {
        &self.model
    }

    fn dimension(&self) -> usize {
        self.dimension
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn deterministic_and_normalized() {
        let provider = MockEmbeddingProvider::new(32, "test".into());
        let first = provider.embed(&["same".into()]).await.unwrap();
        let second = provider.embed(&["same".into()]).await.unwrap();
        assert_eq!(first, second);
        let norm = first[0]
            .iter()
            .map(|value| value * value)
            .sum::<f32>()
            .sqrt();
        assert!((norm - 1.0).abs() < 0.0001);
    }
}
