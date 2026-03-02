use std::collections::{HashMap, HashSet};

#[derive(Debug, Default)]
pub struct WaitForGraph {
    edges: HashMap<u64, HashSet<u64>>,
}

impl WaitForGraph {
    pub fn add_wait(&mut self, waiter_txn_id: u64, holder_txn_id: u64) {
        if waiter_txn_id == holder_txn_id {
            return;
        }
        self.edges
            .entry(waiter_txn_id)
            .or_default()
            .insert(holder_txn_id);
    }

    pub fn clear_waits_for(&mut self, txn_id: u64) {
        self.edges.remove(&txn_id);
    }

    pub fn remove_txn(&mut self, txn_id: u64) {
        self.edges.remove(&txn_id);
        for holders in self.edges.values_mut() {
            holders.remove(&txn_id);
        }
    }

    pub fn cycle_victim_from(&self, start_txn_id: u64) -> Option<u64> {
        let reachable = self.reachable_from(start_txn_id);
        if !reachable.contains(&start_txn_id) {
            return None;
        }
        let sccs = self.strongly_connected_components(&reachable);
        for scc in sccs {
            if !scc.contains(&start_txn_id) {
                continue;
            }
            if scc.len() > 1 {
                return scc.into_iter().max();
            }
            // Single-node SCC is a cycle only if there is an explicit self-loop.
            let node = *scc.iter().next()?;
            if self
                .edges
                .get(&node)
                .is_some_and(|holders| holders.contains(&node))
            {
                return Some(node);
            }
            return None;
        }
        None
    }

    fn reachable_from(&self, start_txn_id: u64) -> HashSet<u64> {
        let mut stack = vec![start_txn_id];
        let mut visited = HashSet::new();
        while let Some(txn_id) = stack.pop() {
            if !visited.insert(txn_id) {
                continue;
            }
            if let Some(next) = self.edges.get(&txn_id) {
                for holder in next {
                    if !visited.contains(holder) {
                        stack.push(*holder);
                    }
                }
            }
        }
        visited
    }

    fn strongly_connected_components(&self, nodes: &HashSet<u64>) -> Vec<HashSet<u64>> {
        fn dfs_order(
            graph: &HashMap<u64, HashSet<u64>>,
            nodes: &HashSet<u64>,
            start: u64,
            visited: &mut HashSet<u64>,
            order: &mut Vec<u64>,
        ) {
            if !visited.insert(start) {
                return;
            }
            if let Some(next) = graph.get(&start) {
                for holder in next {
                    if nodes.contains(holder) {
                        dfs_order(graph, nodes, *holder, visited, order);
                    }
                }
            }
            order.push(start);
        }

        let mut reverse: HashMap<u64, HashSet<u64>> = HashMap::new();
        for node in nodes {
            reverse.entry(*node).or_default();
        }
        for (src, dsts) in &self.edges {
            if !nodes.contains(src) {
                continue;
            }
            for dst in dsts {
                if nodes.contains(dst) {
                    reverse.entry(*dst).or_default().insert(*src);
                }
            }
        }

        let mut visited = HashSet::new();
        let mut order = Vec::new();
        for node in nodes {
            if !visited.contains(node) {
                dfs_order(&self.edges, nodes, *node, &mut visited, &mut order);
            }
        }

        let mut scc_visited = HashSet::new();
        let mut sccs = Vec::new();
        while let Some(node) = order.pop() {
            if !scc_visited.insert(node) {
                continue;
            }
            let mut stack = vec![node];
            let mut component = HashSet::new();
            while let Some(cur) = stack.pop() {
                if !component.insert(cur) {
                    continue;
                }
                if let Some(prev) = reverse.get(&cur) {
                    for p in prev {
                        if !component.contains(p) && scc_visited.insert(*p) {
                            stack.push(*p);
                        }
                    }
                }
            }
            sccs.push(component);
        }
        sccs
    }

    pub fn waits(&self) -> Vec<(u64, u64)> {
        let mut out = Vec::new();
        for (waiter, holders) in &self.edges {
            for holder in holders {
                out.push((*waiter, *holder));
            }
        }
        out.sort_unstable();
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_highest_txn_id_as_victim_for_cycle() {
        let mut graph = WaitForGraph::default();
        graph.add_wait(10, 20);
        graph.add_wait(20, 30);
        graph.add_wait(30, 10);

        assert_eq!(graph.cycle_victim_from(30), Some(30));
        assert_eq!(graph.cycle_victim_from(10), Some(30));
    }

    #[test]
    fn no_cycle_returns_none() {
        let mut graph = WaitForGraph::default();
        graph.add_wait(10, 20);
        graph.add_wait(20, 30);
        assert_eq!(graph.cycle_victim_from(10), None);
    }

    #[test]
    fn waits_snapshot_is_sorted() {
        let mut graph = WaitForGraph::default();
        graph.add_wait(20, 30);
        graph.add_wait(10, 20);
        graph.add_wait(10, 11);
        assert_eq!(graph.waits(), vec![(10, 11), (10, 20), (20, 30)]);
    }

    #[test]
    fn cycle_detection_does_not_abort_reachable_bystander() {
        let mut graph = WaitForGraph::default();
        graph.add_wait(10, 20);
        graph.add_wait(20, 30);
        graph.add_wait(30, 10);
        graph.add_wait(20, 40); // reachable but not in the cycle

        assert_eq!(graph.cycle_victim_from(10), Some(30));
    }
}
