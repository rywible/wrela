#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

use std::collections::HashMap;

pub struct FrameGraph {
    pub passes: Vec<RenderPassConfig>,
}

#[derive(Debug, Clone)]
pub struct RenderPassConfig {
    pub name: String,
    pub kind: PassKind,
    pub dependencies: Vec<String>,
    pub clear_color: Option<[f64; 4]>,
    pub writes_depth: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PassKind {
    Render,
    Compute,
}

impl FrameGraph {
    pub fn new() -> Self {
        FrameGraph { passes: Vec::new() }
    }

    pub fn add_pass(&mut self, config: RenderPassConfig) {
        self.passes.push(config);
    }

    /// Parse a frame graph from a JSON manifest value.
    /// Expects `value["frame_graph"]` to be an array of pass objects.
    pub fn from_manifest(manifest: &serde_json::Value) -> Result<Self, String> {
        let passes_array = manifest
            .get("frame_graph")
            .and_then(|v| v.as_array())
            .ok_or_else(|| "manifest missing 'frame_graph' array".to_string())?;

        let mut passes = Vec::with_capacity(passes_array.len());
        for entry in passes_array {
            let name = entry
                .get("name")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "pass missing 'name'".to_string())?
                .to_string();

            let kind = match entry.get("kind").and_then(|v| v.as_str()) {
                Some("compute") => PassKind::Compute,
                _ => PassKind::Render,
            };

            let dependencies = entry
                .get("dependencies")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();

            let clear_color = entry.get("clear_color").and_then(|v| {
                v.as_array().and_then(|arr| {
                    if arr.len() == 4 {
                        Some([
                            arr[0].as_f64().unwrap_or(0.0),
                            arr[1].as_f64().unwrap_or(0.0),
                            arr[2].as_f64().unwrap_or(0.0),
                            arr[3].as_f64().unwrap_or(1.0),
                        ])
                    } else {
                        None
                    }
                })
            });

            let writes_depth = entry
                .get("writes_depth")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            passes.push(RenderPassConfig {
                name,
                kind,
                dependencies,
                clear_color,
                writes_depth,
            });
        }

        Ok(FrameGraph { passes })
    }

    /// Return passes in topological order based on their declared dependencies.
    /// Returns an error if there is a cycle.
    pub fn sorted_passes(&self) -> Result<Vec<&RenderPassConfig>, String> {
        let name_to_index: HashMap<&str, usize> = self
            .passes
            .iter()
            .enumerate()
            .map(|(i, p)| (p.name.as_str(), i))
            .collect();

        let n = self.passes.len();
        let mut in_degree = vec![0usize; n];
        let mut adjacency: Vec<Vec<usize>> = vec![Vec::new(); n];

        for (i, pass) in self.passes.iter().enumerate() {
            for dep_name in &pass.dependencies {
                if let Some(&dep_index) = name_to_index.get(dep_name.as_str()) {
                    adjacency[dep_index].push(i);
                    in_degree[i] += 1;
                }
                // Unknown dependencies are ignored (external passes)
            }
        }

        let mut queue: Vec<usize> = (0..n).filter(|&i| in_degree[i] == 0).collect();
        let mut order = Vec::with_capacity(n);

        while let Some(node) = queue.pop() {
            order.push(node);
            for &neighbor in &adjacency[node] {
                in_degree[neighbor] -= 1;
                if in_degree[neighbor] == 0 {
                    queue.push(neighbor);
                }
            }
        }

        if order.len() != n {
            return Err("cycle detected in frame graph dependencies".to_string());
        }

        Ok(order.iter().map(|&i| &self.passes[i]).collect())
    }

    /// Build a default forward-rendering frame graph with depth + color passes.
    pub fn default_forward() -> Self {
        FrameGraph {
            passes: vec![
                RenderPassConfig {
                    name: "depth-prepass".to_string(),
                    kind: PassKind::Render,
                    dependencies: Vec::new(),
                    clear_color: None,
                    writes_depth: true,
                },
                RenderPassConfig {
                    name: "forward-opaque".to_string(),
                    kind: PassKind::Render,
                    dependencies: vec!["depth-prepass".to_string()],
                    clear_color: Some([0.05, 0.05, 0.08, 1.0]),
                    writes_depth: true,
                },
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_frame_graph_sorts_to_empty() {
        let fg = FrameGraph::new();
        let sorted = fg.sorted_passes().expect("empty sorts");
        assert!(sorted.is_empty());
    }

    #[test]
    fn single_pass_sorts_to_itself() {
        let mut fg = FrameGraph::new();
        fg.add_pass(RenderPassConfig {
            name: "main".to_string(),
            kind: PassKind::Render,
            dependencies: Vec::new(),
            clear_color: Some([0.0, 0.0, 0.0, 1.0]),
            writes_depth: false,
        });
        let sorted = fg.sorted_passes().expect("single pass sorts");
        assert_eq!(sorted.len(), 1);
        assert_eq!(sorted[0].name, "main");
    }

    #[test]
    fn dependency_order_is_respected() {
        let mut fg = FrameGraph::new();
        fg.add_pass(RenderPassConfig {
            name: "shadow".to_string(),
            kind: PassKind::Render,
            dependencies: Vec::new(),
            clear_color: None,
            writes_depth: true,
        });
        fg.add_pass(RenderPassConfig {
            name: "forward".to_string(),
            kind: PassKind::Render,
            dependencies: vec!["shadow".to_string()],
            clear_color: Some([0.0, 0.0, 0.0, 1.0]),
            writes_depth: true,
        });
        fg.add_pass(RenderPassConfig {
            name: "post".to_string(),
            kind: PassKind::Compute,
            dependencies: vec!["forward".to_string()],
            clear_color: None,
            writes_depth: false,
        });

        let sorted = fg.sorted_passes().expect("DAG sorts");
        let names: Vec<&str> = sorted.iter().map(|p| p.name.as_str()).collect();
        let shadow_pos = names.iter().position(|&n| n == "shadow").unwrap();
        let forward_pos = names.iter().position(|&n| n == "forward").unwrap();
        let post_pos = names.iter().position(|&n| n == "post").unwrap();
        assert!(shadow_pos < forward_pos);
        assert!(forward_pos < post_pos);
    }

    #[test]
    fn cycle_detection_returns_error() {
        let mut fg = FrameGraph::new();
        fg.add_pass(RenderPassConfig {
            name: "a".to_string(),
            kind: PassKind::Render,
            dependencies: vec!["b".to_string()],
            clear_color: None,
            writes_depth: false,
        });
        fg.add_pass(RenderPassConfig {
            name: "b".to_string(),
            kind: PassKind::Render,
            dependencies: vec!["a".to_string()],
            clear_color: None,
            writes_depth: false,
        });

        let result = fg.sorted_passes();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("cycle"));
    }

    #[test]
    fn from_manifest_parses_json() {
        let manifest = serde_json::json!({
            "frame_graph": [
                {
                    "name": "depth",
                    "kind": "render",
                    "dependencies": [],
                    "writes_depth": true
                },
                {
                    "name": "forward",
                    "kind": "render",
                    "dependencies": ["depth"],
                    "clear_color": [0.1, 0.1, 0.1, 1.0],
                    "writes_depth": true
                },
                {
                    "name": "tonemap",
                    "kind": "compute",
                    "dependencies": ["forward"]
                }
            ]
        });

        let fg = FrameGraph::from_manifest(&manifest).expect("parse manifest");
        assert_eq!(fg.passes.len(), 3);
        assert_eq!(fg.passes[0].name, "depth");
        assert!(fg.passes[0].writes_depth);
        assert_eq!(fg.passes[1].kind, PassKind::Render);
        assert!(fg.passes[1].clear_color.is_some());
        assert_eq!(fg.passes[2].kind, PassKind::Compute);
    }

    #[test]
    fn from_manifest_rejects_missing_frame_graph() {
        let manifest = serde_json::json!({});
        let result = FrameGraph::from_manifest(&manifest);
        assert!(result.is_err());
    }

    #[test]
    fn default_forward_graph_has_expected_passes() {
        let fg = FrameGraph::default_forward();
        assert_eq!(fg.passes.len(), 2);
        assert_eq!(fg.passes[0].name, "depth-prepass");
        assert_eq!(fg.passes[1].name, "forward-opaque");
        assert!(fg.passes[1].dependencies.contains(&"depth-prepass".to_string()));

        let sorted = fg.sorted_passes().expect("default forward sorts");
        assert_eq!(sorted[0].name, "depth-prepass");
        assert_eq!(sorted[1].name, "forward-opaque");
    }

    #[test]
    fn parallel_independent_passes_sort_correctly() {
        let mut fg = FrameGraph::new();
        fg.add_pass(RenderPassConfig {
            name: "shadow".to_string(),
            kind: PassKind::Render,
            dependencies: Vec::new(),
            clear_color: None,
            writes_depth: true,
        });
        fg.add_pass(RenderPassConfig {
            name: "reflection".to_string(),
            kind: PassKind::Compute,
            dependencies: Vec::new(),
            clear_color: None,
            writes_depth: false,
        });
        fg.add_pass(RenderPassConfig {
            name: "composite".to_string(),
            kind: PassKind::Render,
            dependencies: vec!["shadow".to_string(), "reflection".to_string()],
            clear_color: Some([0.0, 0.0, 0.0, 1.0]),
            writes_depth: false,
        });

        let sorted = fg.sorted_passes().expect("parallel passes sort");
        assert_eq!(sorted.len(), 3);
        // Composite must come after both shadow and reflection
        let composite_pos = sorted.iter().position(|p| p.name == "composite").unwrap();
        let shadow_pos = sorted.iter().position(|p| p.name == "shadow").unwrap();
        let reflection_pos = sorted.iter().position(|p| p.name == "reflection").unwrap();
        assert!(composite_pos > shadow_pos);
        assert!(composite_pos > reflection_pos);
    }
}
