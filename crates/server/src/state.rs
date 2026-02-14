use std::sync::Arc;
use tokio::sync::RwLock;
use stupid_graph::GraphStore;
use stupid_compute::ComputeEngine;
use stupid_catalog::Catalog;
use stupid_llm::QueryGenerator;

pub type SharedGraph = Arc<RwLock<GraphStore>>;

pub struct AppState {
    pub graph: SharedGraph,
    pub compute: Arc<RwLock<Option<ComputeEngine>>>,
    pub catalog: Catalog,
    pub query_generator: Option<QueryGenerator>,
    pub segment_ids: Vec<String>,
    pub doc_count: u64,
}
