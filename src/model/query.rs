use super::{Graph, NodeId};
use std::collections::BTreeSet;

impl<N> Graph<N> {
    /// Every node reachable by following wires forward from `start`, in id
    /// order. `start` itself appears only if a cycle leads back to it.
    pub fn downstream_nodes(&self, start: NodeId) -> Vec<NodeId> {
        let mut reached = BTreeSet::new();
        let mut frontier = vec![start];
        while let Some(node) = frontier.pop() {
            for wire in self.wires() {
                if wire.from.node == node && reached.insert(wire.to.node) {
                    frontier.push(wire.to.node);
                }
            }
        }
        reached.into_iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ConnectionPolicy, InPin, InputId, OutPin, OutputId};
    use egui::pos2;

    fn link<N>(graph: &mut Graph<N>, from: NodeId, to: NodeId) {
        graph
            .connect(
                OutPin {
                    node: from,
                    output: OutputId(0),
                },
                InPin {
                    node: to,
                    input: InputId(0),
                },
                ConnectionPolicy::Multiple,
            )
            .unwrap();
    }

    #[test]
    fn downstream_follows_wires_forward_only() {
        let mut graph = Graph::new();
        let a = graph.add_node((), pos2(0.0, 0.0));
        let b = graph.add_node((), pos2(0.0, 0.0));
        let c = graph.add_node((), pos2(0.0, 0.0));
        let unrelated = graph.add_node((), pos2(0.0, 0.0));
        link(&mut graph, a, b);
        link(&mut graph, b, c);
        link(&mut graph, unrelated, c);

        let mut downstream = graph.downstream_nodes(a);
        downstream.sort();
        assert_eq!(downstream, vec![b, c]);
        assert!(graph.downstream_nodes(c).is_empty());
        assert_eq!(graph.downstream_nodes(unrelated), vec![c]);
    }

    #[test]
    fn downstream_terminates_on_a_cycle() {
        let mut graph = Graph::new();
        let a = graph.add_node((), pos2(0.0, 0.0));
        let b = graph.add_node((), pos2(0.0, 0.0));
        link(&mut graph, a, b);
        link(&mut graph, b, a);

        let mut downstream = graph.downstream_nodes(a);
        downstream.sort();
        assert_eq!(downstream, vec![a, b], "a reaches itself around the loop");
    }
}
