# Compute Scheduler

## Overview

The scheduler manages execution of compute tasks across the worker pool. It ensures high-priority tasks (P0, P1) run without delay while background tasks (P2, P3) fill idle capacity.

## Scheduling Algorithm

```
loop {
    // 1. Check for P0 tasks (immediate)
    if let Some(task) = p0_queue.try_recv() {
        execute_immediate(task);
        continue;
    }

    // 2. Check for P1 tasks (near-realtime)
    for task in registered_p1_tasks {
        if task.should_run(last_run[task], state) {
            worker_pool.submit(task);
        }
    }

    // 3. Check for P2 tasks (periodic)
    for task in registered_p2_tasks {
        if task.should_run(last_run[task], state) && worker_pool.available() > 2 {
            worker_pool.submit(task);
        }
    }

    // 4. Check for P3 tasks (background)
    for task in registered_p3_tasks {
        if task.should_run(last_run[task], state) && worker_pool.available() > 4 {
            worker_pool.submit(task);
        }
    }

    // 5. Sleep briefly if nothing to do
    sleep(Duration::from_millis(100));
}
```

## Backpressure Handling

When the system is under load (ingest backlog):

1. **P3 tasks are paused** — free all workers for ingest/connect
2. **P2 tasks are delayed** — run at half frequency
3. **P1 tasks continue** — they're cheap and keep knowledge fresh
4. **P0 tasks always run** — they're part of the ingest pipeline

```rust
enum LoadLevel {
    Normal,    // All priorities active
    Elevated,  // P3 paused, P2 half frequency
    Critical,  // P2+P3 paused, only P0+P1
}

fn assess_load(ingest_queue_depth: usize) -> LoadLevel {
    match ingest_queue_depth {
        0..=1000 => LoadLevel::Normal,
        1001..=10000 => LoadLevel::Elevated,
        _ => LoadLevel::Critical,
    }
}
```

## Task Dependencies

Some tasks depend on others:

```
Entity extraction (P0) → Edge creation (P0) → PageRank (P2)
                                              → Louvain (P2)

Embedding generation (P0) → Streaming K-means (P0)
                           → DBSCAN (P1)
                           → Full K-means (P3)

Co-occurrence update (P1) → Pattern mining (P3)
```

The scheduler respects these: a P2 task won't run if its P0/P1 dependencies haven't completed for the current batch.

## Monitoring

The scheduler exposes metrics:

```rust
struct SchedulerMetrics {
    tasks_executed: HashMap<String, u64>,
    tasks_pending: HashMap<Priority, usize>,
    worker_utilization: f64,           // 0.0 - 1.0
    avg_task_duration: HashMap<String, Duration>,
    last_run: HashMap<String, DateTime<Utc>>,
    current_load_level: LoadLevel,
    ingest_queue_depth: usize,
}
```

These are exposed to the dashboard via WebSocket for the system health panel.

## Configuration

```toml
[compute.scheduler]
worker_threads = 0        # 0 = num_cpus
p1_interval_seconds = 300 # 5 minutes
p2_interval_seconds = 3600 # 1 hour
p3_interval_seconds = 86400 # 1 day
backpressure_threshold = 1000
critical_threshold = 10000
```
