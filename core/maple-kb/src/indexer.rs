use serde::{Deserialize, Serialize};

use anyhow::Result;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    pub id: String,
    pub title: String,
    pub content: String,
    pub source_type: String,
    pub source_url: Option<String>,
    pub chunks: Vec<DocumentChunk>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentChunk {
    pub id: String,
    pub document_id: String,
    pub content: String,
    pub index: usize,
    pub embedding: Option<Vec<f32>>,
}

pub struct Indexer {
    chunk_size: usize,
    chunk_overlap: usize,
}

impl Indexer {
    pub fn new(chunk_size: usize, chunk_overlap: usize) -> Self {
        Self { chunk_size, chunk_overlap }
    }

    pub fn index(&self, doc: &Document) -> Result<Vec<DocumentChunk>> {
        let content = &doc.content;
        let mut chunks = Vec::new();
        let mut start = 0;
        let mut index = 0;

        while start < content.len() {
            let end = (start + self.chunk_size).min(content.len());
            let chunk_content = content[start..end].to_string();

            chunks.push(DocumentChunk {
                id: format!("{}_{}", doc.id, index),
                document_id: doc.id.clone(),
                content: chunk_content,
                index,
                embedding: None,
            });

            start += self.chunk_size - self.chunk_overlap;
            index += 1;
        }

        Ok(chunks)
    }
}
