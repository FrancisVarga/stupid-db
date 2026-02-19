use std::collections::{HashMap, VecDeque};
use std::path::Path;

use crate::error::EisenbahnError;
use crate::transport::Transport;

use super::types::StageConfig;

/// Parse an endpoint string like "ipc:///tmp/foo.sock" or "tcp://host:port" into a Transport.
pub(crate) fn parse_endpoint_to_transport(endpoint: &str) -> Transport {
    if let Some(path) = endpoint.strip_prefix("ipc://") {
        let name = Path::new(path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown");
        Transport::ipc(name)
    } else if let Some(addr) = endpoint.strip_prefix("tcp://") {
        if let Some((host, port_str)) = addr.rsplit_once(':') {
            let port = port_str.parse().unwrap_or(5555);
            Transport::tcp(host, port)
        } else {
            Transport::tcp(addr, 5555)
        }
    } else {
        Transport::ipc("unknown")
    }
}

/// Topological sort using Kahn's algorithm.
///
/// Returns stage names in dependency order, or an error if a cycle is detected.
pub(crate) fn topological_sort(
    stages: &HashMap<String, StageConfig>,
) -> Result<Vec<String>, EisenbahnError> {
    if stages.is_empty() {
        return Ok(Vec::new());
    }

    // Build adjacency list and in-degree map
    let mut in_degree: HashMap<&str, usize> = HashMap::new();
    let mut dependents: HashMap<&str, Vec<&str>> = HashMap::new();

    for name in stages.keys() {
        in_degree.entry(name.as_str()).or_insert(0);
        dependents.entry(name.as_str()).or_default();
    }

    for (name, stage) in stages {
        for dep in &stage.after {
            dependents
                .entry(dep.as_str())
                .or_default()
                .push(name.as_str());
            *in_degree.entry(name.as_str()).or_insert(0) += 1;
        }
    }

    let mut queue: VecDeque<&str> = in_degree
        .iter()
        .filter(|(_, &deg)| deg == 0)
        .map(|(&name, _)| name)
        .collect();

    let mut sorted = Vec::with_capacity(stages.len());

    while let Some(node) = queue.pop_front() {
        sorted.push(node.to_string());
        if let Some(deps) = dependents.get(node) {
            for &dep in deps {
                if let Some(deg) = in_degree.get_mut(dep) {
                    *deg -= 1;
                    if *deg == 0 {
                        queue.push_back(dep);
                    }
                }
            }
        }
    }

    if sorted.len() != stages.len() {
        let in_cycle: Vec<&str> = in_degree
            .iter()
            .filter(|(_, &deg)| deg > 0)
            .map(|(&name, _)| name)
            .collect();
        return Err(EisenbahnError::CircularDependency(format!(
            "cycle detected among stages: {}",
            in_cycle.join(" → ")
        )));
    }

    Ok(sorted)
}
