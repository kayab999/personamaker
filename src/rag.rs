/// Retrieves the top K most relevant chunks for a given query.
///
/// Returns an error when chunks exist but **none** have embeddings (legacy ingest),
/// so callers can surface reindex needs instead of silent empty retrieval.
pub fn retrieve_relevant_chunks(
    all_chunks: &[crate::storage::KnowledgeChunk],
    query: &str,
    k: usize,
) -> Result<Vec<crate::storage::KnowledgeChunk>> {
    if all_chunks.is_empty() {
        return Ok(vec![]);
    }

    let with_emb = all_chunks.iter().filter(|c| c.embedding.is_some()).count();
    if with_emb == 0 {
        return Err(anyhow!(
            "Knowledge chunks exist but none have embeddings (legacy or failed ingest). Re-upload documents to reindex."
        ));
    }

    let query_embedding = embed_texts(vec![query.to_string()])?
        .first()
        .cloned()
        .ok_or_else(|| anyhow!("Failed to generate embedding for query"))?;

    let mut scored_chunks: Vec<(&crate::storage::KnowledgeChunk, f32)> = all_chunks
        .iter()
        .filter_map(|chunk| {
            chunk.embedding.as_ref().and_then(|emb| {
                let a = emb.as_slice();
                let b = query_embedding.as_slice();
                if a.len() != b.len() {
                    return None;
                }
                Some((chunk, cosine_similarity(a, b)))
            })
        })
        .collect();

    scored_chunks.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    // Filter out very low relevance chunks (helps keep context clean and reduces noise)
    const MIN_SIMILARITY: f32 = 0.25;
    let filtered: Vec<_> = scored_chunks
        .into_iter()
        .filter(|(_, score)| *score >= MIN_SIMILARITY)
        .take(k)
        .map(|(c, _)| c.clone())
        .collect();

    Ok(filtered)
}

use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};
use anyhow::{anyhow, Result};
use std::sync::Mutex;
use once_cell::sync::Lazy;

/// Global model instance (lazy initialized, protected by Mutex because embed requires &mut self).
/// Phase 1.5: Uses Option so failures are not permanent — next call retries init.
/// C03: catch_unwind wraps the lock acquisition to recover from poisoning.
static EMBEDDING_MODEL: Lazy<Mutex<Option<TextEmbedding>>> = Lazy::new(|| {
    Mutex::new(None)
});

/// Computes embeddings for a list of strings.
/// C03: Uses catch_unwind to recover from Mutex poisoning.
pub fn embed_texts(texts: Vec<String>) -> Result<Vec<Vec<f32>>> {
    use std::panic::AssertUnwindSafe;
    let guard_result = std::panic::catch_unwind(AssertUnwindSafe(|| EMBEDDING_MODEL.lock()));
    let mut guard = match guard_result {
        Ok(Ok(g)) => g,
        Ok(Err(poisoned)) => {
            log::warn!("Embedding model Mutex was poisoned, recovering...");
            poisoned.into_inner()
        }
        Err(panic_err) => {
            let msg = if let Some(s) = panic_err.downcast_ref::<&str>() { s.to_string() }
                      else { "Unknown panic".to_string() };
            log::error!("Panic while locking embedding model: {}", msg);
            // Mutex is corrupted, recreate it
            let guard = EMBEDDING_MODEL.lock().unwrap_or_else(|e| e.into_inner());
            drop(guard); // Release the lock, model will be re-initialized on next call
            return Err(anyhow!("Embedding model panic recovered: {}", msg));
        }
    };
    if guard.is_none() {
        match TextEmbedding::try_new(InitOptions::new(EmbeddingModel::AllMiniLML6V2)
            .with_show_download_progress(true))
        {
            Ok(model) => {
                log::info!("Embedding model initialized successfully");
                *guard = Some(model);
            }
            Err(e) => {
                log::warn!("Failed to initialize embedding model (will retry on next call): {}", e);
                return Err(anyhow!("Failed to initialize embedding model: {}", e));
            }
        }
    }
    let model = guard.as_mut().ok_or_else(|| anyhow!("Embedding model not available"))?;
    model.embed(texts, None).map_err(|e| anyhow!("Failed to generate embeddings: {}", e))
}

/// Simple cosine similarity between two vectors
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b.iter()).map(|(ai, bi)| ai * bi).sum();
    let norm_a: f32 = a.iter().map(|ai| ai * ai).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|bi| bi * bi).sum::<f32>().sqrt();
    
    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }
    dot / (norm_a * norm_b)
}
