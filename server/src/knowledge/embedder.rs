use async_trait::async_trait;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum EmbeddingError {
    #[error("embedding configuration error: {0}")]
    Configuration(String),
    #[error("embedding request failed: {0}")]
    Request(String),
    #[error("embedding provider returned invalid output: {0}")]
    InvalidOutput(String),
}

#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbeddingError>;
    fn provider_name(&self) -> &str;
    fn model_name(&self) -> &str;
    fn dimension(&self) -> usize;
}

pub fn validate_embeddings(
    embeddings: &[Vec<f32>],
    expected_count: usize,
    expected_dimension: usize,
) -> Result<(), EmbeddingError> {
    if embeddings.len() != expected_count {
        return Err(EmbeddingError::InvalidOutput(format!(
            "expected {expected_count} vectors, got {}",
            embeddings.len()
        )));
    }
    for (index, embedding) in embeddings.iter().enumerate() {
        if embedding.len() != expected_dimension {
            return Err(EmbeddingError::InvalidOutput(format!(
                "vector {index} has dimension {}, expected {expected_dimension}",
                embedding.len()
            )));
        }
        if embedding.iter().any(|value| !value.is_finite()) {
            return Err(EmbeddingError::InvalidOutput(format!(
                "vector {index} contains a non-finite value"
            )));
        }
    }
    Ok(())
}

pub fn vector_literal(values: &[f32]) -> Result<String, EmbeddingError> {
    if values.iter().any(|value| !value.is_finite()) {
        return Err(EmbeddingError::InvalidOutput(
            "cannot serialize non-finite vector".into(),
        ));
    }
    let mut literal = String::with_capacity(values.len() * 12 + 2);
    literal.push('[');
    for (index, value) in values.iter().enumerate() {
        if index > 0 {
            literal.push(',');
        }
        literal.push_str(&value.to_string());
    }
    literal.push(']');
    Ok(literal)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_wrong_dimension_and_non_finite_values() {
        assert!(validate_embeddings(&[vec![0.0]], 1, 2).is_err());
        assert!(validate_embeddings(&[vec![f32::NAN, 0.0]], 1, 2).is_err());
    }
}
