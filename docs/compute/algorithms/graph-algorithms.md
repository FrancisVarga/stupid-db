# Graph Algorithms

## Overview

Graph algorithms run on the in-memory property graph to discover structure, influence, and communities in the member-entity network.

## PageRank

### Purpose
Identify the most **influential** or **central** entities in the graph. High PageRank members are well-connected hubs; high PageRank games are popular across segments.

### Algorithm

Standard PageRank with damping factor:

```
PR(v) = (1 - d) / N + d * SUM(PR(u) / out_degree(u)) for all u linking to v
```

Where:
- `d` = 0.85 (damping factor)
- `N` = total nodes
- Converges after ~20-50 iterations

### Configuration

| Parameter | Default | Description |
|-----------|---------|-------------|
| `damping` | 0.85 | Probability of following a link vs random jump |
| `max_iterations` | 50 | Maximum iterations before stopping |
| `convergence_threshold` | 1e-6 | Stop when max delta < threshold |
| `edge_weight` | true | Use edge weights (occurrence count) |

### Execution (P2, hourly)

```mermaid
flowchart TD
    A[Read Current Graph Snapshot] --> B[Initialize PR = 1/N for All Nodes]
    B --> C[Iterate PageRank Formula]
    C --> D{Converged?}
    D -->|No| C
    D -->|Yes| E[Normalize Scores]
    E --> F[Store Per-Entity-Type Rankings]
    F --> G[Detect Rank Changes vs Previous Run]
    G --> H{Significant Change?}
    H -->|Yes| I[Push Insight: Rank Shift Detected]
    H -->|No| J[Done]
```

### Output

```rust
struct PageRankResult {
    scores: HashMap<NodeId, f64>,
    // Top entities per type
    top_members: Vec<(NodeId, f64)>,     // Most connected members
    top_games: Vec<(NodeId, f64)>,       // Most popular games
    top_devices: Vec<(NodeId, f64)>,     // Most common devices
    rank_changes: Vec<RankChange>,       // Significant moves since last run
    iterations: usize,
    converged: bool,
}
```

### Weighted PageRank

Edge weights (occurrence counts) are incorporated:

- Member logs in from Device 50 times → edge weight 50
- Member opens Game once → edge weight 1
- PageRank flows more through high-weight edges
- This means "primary device" and "favorite game" contribute more

## Label Propagation Community Detection

### Purpose
Discover **natural communities** — groups of entities that are more densely connected to each other than to the rest of the graph. Think: "these 500 members form a cohesive group based on their game choices, devices, and VIP tiers."

### Algorithm

Label Propagation:

1. Start: assign each node a unique label (0..N)
2. For each node, adopt the most frequent label among its neighbors
3. Break ties by choosing the smallest label (deterministic)
4. Repeat until no labels change or max iterations reached

This is a fast, near-linear-time algorithm that naturally discovers connected components and densely-connected groups without requiring a modularity function.

### Configuration

| Parameter | Default | Description |
|-----------|---------|-------------|
| `max_iterations` | 10 | Maximum iterations before stopping |

### Execution (P2, hourly)

```mermaid
flowchart TD
    A[Read Graph Snapshot] --> B[Initialize: Each Node Gets Unique Label 0..N]
    B --> C[For Each Node: Adopt Most Frequent Neighbor Label]
    C --> D{Any Labels Changed?}
    D -->|Yes| E{Max Iterations?}
    E -->|No| C
    E -->|Yes| F[Return Current Labels]
    D -->|No| F
    F --> G[Store Community Assignments in KnowledgeState]
```

### Output

```rust
// Returns HashMap<NodeId, CommunityId> where CommunityId is u64
fn label_propagation(graph: &GraphStore, max_iterations: usize) -> HashMap<NodeId, u64>
fn label_propagation_default(graph: &GraphStore) -> HashMap<NodeId, u64>  // max_iterations=10
```

The scheduler task (`CommunityDetectionTask`, P2) wraps this algorithm and writes results to `KnowledgeState.communities`. The API endpoint `/compute/communities` groups results by community ID and returns member counts with top node samples.

## Shortest Path

### Purpose
Answer "how are entity A and entity B connected?" — used by the query layer for ad-hoc relationship exploration.

### Algorithm
Weighted Dijkstra with edge weights inverted (high weight = short distance, because frequent connections are "closer").

### Execution: On-demand (not scheduled)

```rust
fn shortest_path(from: NodeId, to: NodeId) -> Option<Vec<(NodeId, EdgeId)>>
```

Used in queries like: "How is member X connected to game Y?" → shows the path through the graph.

## Graph Statistics

Maintained continuously and exposed to the dashboard:

```rust
struct GraphStats {
    total_nodes: usize,
    total_edges: usize,
    nodes_by_type: HashMap<EntityType, usize>,
    edges_by_type: HashMap<EdgeType, usize>,
    avg_degree: f64,
    max_degree: (NodeId, usize),
    connected_components: usize,
    density: f64,
}
```
