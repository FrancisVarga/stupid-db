---
name: Graph Algorithms
description: Graph algorithm patterns for stupid-db's in-memory property graph. PageRank, Louvain community detection, BFS/DFS traversal, graph statistics, and segment-aware edge indexing. Use when working on graph algorithms or graph store.
version: 1.0.0
---

# Graph Algorithms

## Property Graph Model

```rust
pub struct GraphStore {
    nodes: HashMap<NodeId, Node>,
    edges: HashMap<EdgeId, Edge>,
    adjacency: HashMap<NodeId, Vec<EdgeId>>,     // outgoing
    reverse_adj: HashMap<NodeId, Vec<EdgeId>>,    // incoming
    segment_index: HashMap<SegmentId, Vec<EdgeId>>, // for eviction
}

pub struct Node {
    pub id: NodeId,
    pub entity_type: EntityType,
    pub properties: HashMap<String, FieldValue>,
}

pub struct Edge {
    pub id: EdgeId,
    pub edge_type: EdgeType,
    pub source: NodeId,
    pub target: NodeId,
    pub weight: f64,
    pub segment_id: SegmentId, // for eviction tracking
    pub timestamp: DateTime<Utc>,
}
```

**Segment-aware edges**: Every edge carries `segment_id` so when a segment is evicted, its edges can be efficiently removed.

## PageRank

**Purpose**: Identify influential entities (high-traffic members, popular games).

```rust
pub fn pagerank(
    graph: &GraphStore,
    damping: f64,      // typically 0.85
    max_iter: usize,   // typically 100
    tolerance: f64,     // convergence: 1e-6
) -> HashMap<NodeId, f64>
```

### Algorithm
1. Initialize all nodes with rank = 1/N
2. For each iteration:
   - For each node, sum incoming edge weights × source rank
   - Apply damping: `rank = (1 - d)/N + d * sum`
   - Check convergence: max rank change < tolerance
3. Return final ranks

### When to Use
- "Which members are most active?" → PageRank on member nodes
- "Which games are most popular?" → PageRank weighted by play edges
- "Most connected entities?" → PageRank identifies network hubs

### Tuning
- Higher damping (0.9) → more sensitive to graph structure
- Lower damping (0.7) → more uniform distribution
- On large graphs, limit iterations to prevent long compute

## Louvain Community Detection

**Purpose**: Discover natural clusters of densely connected entities.

```rust
pub fn louvain(
    graph: &GraphStore,
    resolution: f64,   // granularity: 1.0 = standard
    max_iter: usize,
) -> Vec<Community>

pub struct Community {
    pub id: CommunityId,
    pub members: Vec<NodeId>,
    pub modularity_contribution: f64,
}
```

### Algorithm
1. Start: each node in its own community
2. Phase 1 (local moves): Move each node to neighbor's community if modularity improves
3. Phase 2 (aggregation): Merge communities into super-nodes
4. Repeat until no improvement

### When to Use
- "Group similar members" → communities of members who play same games
- "Find game clusters" → games played by same member groups
- "Detect fraud rings" → tightly connected member-device clusters

### Tuning
- Resolution < 1.0 → fewer, larger communities
- Resolution > 1.0 → more, smaller communities
- For fraud detection, higher resolution finds tighter clusters

## Traversal Algorithms

### BFS (Breadth-First Search)
```rust
pub fn bfs(
    graph: &GraphStore,
    start: NodeId,
    max_depth: usize,
    edge_filter: Option<EdgeType>,
) -> Vec<(NodeId, usize)> // (node, depth)
```

Use case: "Find all entities within N hops of member X"

### DFS (Depth-First Search)
```rust
pub fn dfs(
    graph: &GraphStore,
    start: NodeId,
    max_depth: usize,
    visitor: &mut dyn FnMut(&Node, usize) -> bool, // return false to prune
) -> Vec<NodeId>
```

Use case: "Explore paths from member to error codes"

### Shortest Path (Dijkstra)
```rust
pub fn shortest_path(
    graph: &GraphStore,
    source: NodeId,
    target: NodeId,
    weight_fn: impl Fn(&Edge) -> f64,
) -> Option<Vec<NodeId>>
```

Use case: "How is member A connected to member B?"

## Graph Statistics

```rust
pub struct GraphStats {
    pub node_count: usize,
    pub edge_count: usize,
    pub density: f64,
    pub avg_degree: f64,
    pub degree_distribution: HashMap<usize, usize>, // degree → count
    pub connected_components: usize,
    pub largest_component_size: usize,
}
```

Used by the catalog to provide graph health metrics to the LLM.

## Performance Notes

- All algorithms operate on the in-memory graph (fast, no I/O)
- For large graphs (millions of edges), PageRank and Louvain use `rayon` for parallel iteration
- Segment-aware eviction is O(edges_in_segment) not O(all_edges)
- BFS/DFS should always have `max_depth` to prevent unbounded traversal
