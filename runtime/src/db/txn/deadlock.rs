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
        let mut stack = vec![start_txn_id];
        let mut visited = HashSet::new();
        let mut in_cycle = HashSet::new();

        while let Some(txn_id) = stack.pop() {
            if !visited.insert(txn_id) {
                continue;
            }
            if let Some(next) = self.edges.get(&txn_id) {
                for holder in next {
                    if *holder == start_txn_id {
                        in_cycle.insert(start_txn_id);
                        in_cycle.insert(txn_id);
                    }
                    if !visited.contains(holder) {
                        stack.push(*holder);
                    }
                }
            }
        }

        if in_cycle.is_empty() {
            return None;
        }

        for &txn_id in &visited {
            if let Some(next) = self.edges.get(&txn_id) {
                for holder in next {
                    if visited.contains(holder) {
                        in_cycle.insert(txn_id);
                        in_cycle.insert(*holder);
                    }
                }
            }
        }

        in_cycle.into_iter().max()
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
}
