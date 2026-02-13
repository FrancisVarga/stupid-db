# Graph Store

## Overview

The graph store maintains an in-memory property graph of entities and their relationships. It is automatically populated by the ingestion pipeline — no direct writes from users. The graph enables traversal queries, community detection, influence analysis, and relationship discovery.

## Data Model

### Nodes

```rust
struct Node {
    id: NodeId,
    entity_type: EntityType,
    key: String,                      // Natural key: "memberCode:thongtran2904"
    properties: HashMap<String, Value>,
    segment_refs: HashSet<SegmentId>, // Which segments have docs for this entity
    created_at: DateTime<Utc>,
    last_seen: DateTime<Utc>,
}

enum EntityType {
    Member,       // memberCode
    Device,       // fingerprint
    Game,         // gameUid
    Affiliate,    // affiliateId
    Currency,     // currency
    VipGroup,     // rGroup
    Error,        // error+stage
    Platform,     // platform
    Popup,        // componentId
    Provider,     // gameTrackingProvider (GPI, etc.)
}
```

### Edges

```rust
struct Edge {
    id: EdgeId,
    source: NodeId,
    target: NodeId,
    edge_type: EdgeType,
    weight: f64,                      // Incremented on repeated occurrence
    first_seen: DateTime<Utc>,
    last_seen: DateTime<Utc>,
    segment_id: SegmentId,            // Which segment created this edge
    properties: HashMap<String, Value>,
}

enum EdgeType {
    LoggedInFrom,     // Member → Device
    OpenedGame,       // Member → Game
    SawPopup,         // Member → Popup
    HitError,         // Member → Error
    BelongsToGroup,   // Member → VipGroup
    ReferredBy,       // Member → Affiliate
    UsesCurrency,     // Member → Currency
    PlaysOnPlatform,  // Member → Platform
    ProvidedBy,       // Game → Provider
    // Computed edges (from warm path)
    SimilarTo,        // Member → Member (vector similarity)
    CoOccursWith,     // Game → Game (co-occurrence)
    InCluster,        // Member → Cluster (K-means assignment)
    InCommunity,      // Member → Community (Louvain assignment)
}
```

## Storage Structure

### In-Memory Adjacency

```rust
struct GraphStore {
    nodes: HashMap<NodeId, Node>,
    // Entity key → NodeId for dedup
    key_index: HashMap<(EntityType, String), NodeId>,
    // Adjacency lists (both directions for bidirectional traversal)
    outgoing: HashMap<NodeId, Vec<EdgeId>>,
    incoming: HashMap<NodeId, Vec<EdgeId>>,
    edges: HashMap<EdgeId, Edge>,
    // Segment index for eviction
    segment_edges: HashMap<SegmentId, Vec<EdgeId>>,
}
```

### Memory Estimate

For 30 days of data:

```
Unique members: ~500K (estimate from 57K logins/day with repeats)
Unique devices: ~300K
Unique games: ~1K
Other entities: ~10K

Total nodes: ~811K
Total edges: ~30M (members × events × entity connections)

Per node: ~200 bytes → ~162 MB
Per edge: ~120 bytes → ~3.6 GB
Adjacency lists: ~480 MB
Index structures: ~200 MB

Total: ~4.5 GB
```

Fits comfortably in memory.

## Operations

### Upsert Node

```rust
fn upsert_node(entity_type: EntityType, key: &str, properties: HashMap<String, Value>) -> NodeId
```

- If node with `(entity_type, key)` exists, update `last_seen` and merge properties
- If not, create new node
- Returns NodeId (new or existing)

### Add Edge

```rust
fn add_edge(source: NodeId, target: NodeId, edge_type: EdgeType, segment_id: SegmentId) -> EdgeId
```

- If edge `(source, target, edge_type)` already exists, increment weight and update `last_seen`
- If not, create new edge
- Register edge in `segment_edges` for eviction tracking

### Traverse

```rust
fn traverse(
    start: NodeId,
    edge_types: &[EdgeType],
    direction: Direction, // Outgoing, Incoming, Both
    depth: usize,
    filter: Option<NodeFilter>,
) -> Vec<(NodeId, Vec<EdgeId>, usize)> // (node, path, depth)
```

BFS/DFS traversal from a starting node. Used by query plans.

### Neighborhood

```rust
fn neighborhood(node: NodeId, depth: usize) -> SubGraph
```

Extract a subgraph around a node. Used by the dashboard for graph visualization.

### Evict Segment

```rust
fn evict_segment(segment_id: SegmentId)
```

1. Look up all edges in `segment_edges[segment_id]`
2. Remove those edges from adjacency lists
3. Remove edges from `edges` map
4. For nodes that now have zero edges and `segment_refs` only contained this segment, remove the node
5. Remove `segment_edges[segment_id]` entry

**Complexity**: O(edges_in_segment) — efficient because of the segment index.

## Edge Weight Semantics

Edges have a `weight` field that increases each time the relationship is observed:

- Member X logs in from Device Y once → weight = 1
- Member X logs in from Device Y 50 times in 30 days → weight = 50

This allows distinguishing strong relationships (daily login) from weak ones (one-time event). Graph algorithms (PageRank, community detection) use these weights.

## Concurrency

- **Single writer** — the hot-path connector is the sole writer
- **Multiple readers** — query execution, compute algorithms, dashboard API
- Implementation: `RwLock<GraphStore>` or lock-free with epoch-based reclamation
- Write batching: connector accumulates edges during a batch ingest, then applies them in a single write lock acquisition
