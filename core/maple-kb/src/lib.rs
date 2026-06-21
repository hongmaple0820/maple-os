pub mod indexer;
pub mod retriever;
pub mod evolver;
pub mod memory;
pub mod prompt_version;
pub mod bm25;
pub mod vector_store;
pub mod parser;
pub mod learning_governance;

pub use retriever::HybridRetriever;
pub use evolver::{Evolver, EvolutionConfig, DistillationResult, EvolutionStats, KnowledgeLink};
pub use memory::MemoryStore;
pub use bm25::BM25Searcher;
pub use vector_store::{VectorSearch, InMemoryVectorStore, QdrantVectorStore, ChunkEmbedding};
pub use indexer::Indexer;
pub use prompt_version::PromptVersionManager;
pub use parser::{DocumentParserRegistry, DocumentParser, TextParser, PdfParser};
pub use learning_governance::{LearningGovernanceService, LearningCandidate, CreateCandidateRequest, CandidateOutcome};
