use std::collections::{HashMap, HashSet};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RagChunk {
    pub id: String,
    pub source: String,
    pub content: String,
    pub score: f32,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum RagError {
    #[error("invalid retrieval chunk")]
    InvalidChunk,
    #[error("retrieval failed")]
    Failed,
}

#[async_trait]
pub trait RagRetriever: Send + Sync {
    async fn retrieve(&self, query: &str, limit: usize) -> Result<Vec<RagChunk>, RagError>;
}

#[derive(Debug, Default)]
pub struct NoopRagRetriever;

#[async_trait]
impl RagRetriever for NoopRagRetriever {
    async fn retrieve(&self, _: &str, _: usize) -> Result<Vec<RagChunk>, RagError> {
        Ok(Vec::new())
    }
}

#[derive(Debug, Clone)]
struct IndexedChunk {
    id: String,
    source: String,
    content: String,
    terms: HashSet<String>,
}

#[derive(Debug, Clone, Default)]
pub struct InMemoryRagIndex {
    chunks: Vec<IndexedChunk>,
}

impl InMemoryRagIndex {
    pub fn new(chunks: Vec<RagChunk>) -> Result<Self, RagError> {
        let mut ids = HashSet::new();
        let mut indexed = Vec::with_capacity(chunks.len());
        for chunk in chunks {
            validate_chunk(&chunk, 64 * 1024)?;
            if !ids.insert(chunk.id.clone()) {
                return Err(RagError::InvalidChunk);
            }
            indexed.push(IndexedChunk {
                terms: tokenize(&chunk.content),
                id: chunk.id,
                source: chunk.source,
                content: chunk.content,
            });
        }
        indexed.sort_by(|left, right| left.id.cmp(&right.id));
        Ok(Self { chunks: indexed })
    }
}

#[async_trait]
impl RagRetriever for InMemoryRagIndex {
    async fn retrieve(&self, query: &str, limit: usize) -> Result<Vec<RagChunk>, RagError> {
        if limit == 0 {
            return Ok(Vec::new());
        }
        let query_terms = tokenize(query);
        if query_terms.is_empty() {
            return Ok(Vec::new());
        }
        let mut document_frequency = HashMap::<&str, usize>::new();
        for chunk in &self.chunks {
            for term in &chunk.terms {
                *document_frequency.entry(term).or_default() += 1;
            }
        }
        let mut matches = self
            .chunks
            .iter()
            .filter_map(|chunk| {
                let score = query_terms
                    .iter()
                    .filter(|term| chunk.terms.contains(*term))
                    .map(|term| {
                        let frequency = document_frequency.get(term.as_str()).copied().unwrap_or(1);
                        1.0_f32 + ((self.chunks.len() + 1) as f32 / frequency as f32).ln()
                    })
                    .sum::<f32>();
                (score > 0.0).then(|| RagChunk {
                    id: chunk.id.clone(),
                    source: chunk.source.clone(),
                    content: chunk.content.clone(),
                    score,
                })
            })
            .collect::<Vec<_>>();
        matches.sort_by(|left, right| {
            right
                .score
                .total_cmp(&left.score)
                .then_with(|| left.id.cmp(&right.id))
        });
        matches.truncate(limit);
        Ok(matches)
    }
}

pub(crate) fn validate_chunk(chunk: &RagChunk, max_content_bytes: usize) -> Result<(), RagError> {
    let valid_id = !chunk.id.is_empty()
        && chunk.id.len() <= 128
        && chunk
            .id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'));
    if !valid_id
        || chunk.source.is_empty()
        || chunk.source.len() > 1024
        || chunk.source.contains('\0')
        || chunk.content.is_empty()
        || chunk.content.len() > max_content_bytes
        || chunk.content.contains('\0')
        || !chunk.score.is_finite()
        || chunk.score < 0.0
    {
        Err(RagError::InvalidChunk)
    } else {
        Ok(())
    }
}

fn tokenize(value: &str) -> HashSet<String> {
    value
        .split(|character: char| !character.is_alphanumeric() && character != '_')
        .filter(|token| !token.is_empty())
        .map(str::to_lowercase)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk(id: &str, content: &str) -> RagChunk {
        RagChunk {
            id: id.into(),
            source: format!("source/{id}"),
            content: content.into(),
            score: 0.0,
        }
    }

    #[tokio::test]
    async fn lexical_results_are_ranked_bounded_and_stable() {
        let index = InMemoryRagIndex::new(vec![
            chunk("b", "rust memory"),
            chunk("a", "rust memory"),
            chunk("c", "unrelated"),
            chunk("d", "rust only"),
        ])
        .unwrap();
        let results = index.retrieve("rust memory missing", 3).await.unwrap();
        assert_eq!(
            results
                .iter()
                .map(|chunk| chunk.id.as_str())
                .collect::<Vec<_>>(),
            vec!["a", "b", "d"]
        );
        assert!(results.iter().all(|chunk| chunk.score > 0.0));
        assert!(index.retrieve("zero overlap", 5).await.unwrap().is_empty());
        assert!(index.retrieve("rust", 0).await.unwrap().is_empty());
    }

    #[test]
    fn invalid_and_duplicate_chunks_fail_closed() {
        assert_eq!(
            InMemoryRagIndex::new(vec![chunk("../bad", "text")]).unwrap_err(),
            RagError::InvalidChunk
        );
        assert_eq!(
            InMemoryRagIndex::new(vec![chunk("same", "one"), chunk("same", "two")]).unwrap_err(),
            RagError::InvalidChunk
        );
    }
}
