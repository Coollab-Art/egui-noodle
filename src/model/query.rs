use super::{Graph, InPin, InputId, NodeId, OutPin, OutputId, Wire};
use std::collections::{BTreeMap, BTreeSet};

impl<N> Graph<N> {
    /// Every node reachable by following wires forward from `start`, in id
    /// order. `start` itself appears only if a cycle leads back to it.
    pub fn downstream_nodes(&self, start: NodeId) -> Vec<NodeId> {
        self.reachable(start, |wire| (wire.from.node, wire.to.node))
    }

    /// Every node reachable by following wires backward from `start`, in id
    /// order. `start` itself appears only if a cycle leads back to it.
    pub fn upstream_nodes(&self, start: NodeId) -> Vec<NodeId> {
        self.reachable(start, |wire| (wire.to.node, wire.from.node))
    }

    fn reachable(&self, start: NodeId, step: impl Fn(&Wire) -> (NodeId, NodeId)) -> Vec<NodeId> {
        let mut reached = BTreeSet::new();
        let mut frontier = vec![start];
        while let Some(node) = frontier.pop() {
            for wire in self.wires() {
                let (here, next) = step(wire);
                if here == node && reached.insert(next) {
                    frontier.push(next);
                }
            }
        }
        reached.into_iter().collect()
    }

    /// The wires that close the chains `removed` nodes sit in: for every node
    /// removed with pass-through pins `(input, output)`, whatever feeds its
    /// input gets wired to whatever its output feeds - through any other
    /// removed node in between, so deleting `B` and `C` out of `A -> B -> C -> D`
    /// yields `A -> D`. Wires already present, and wires that would loop a
    /// node into itself, are left out. A removed node with no pass-through
    /// pins breaks its chain.
    pub fn bridges_over(
        &self,
        removed: &BTreeMap<NodeId, Option<(InputId, OutputId)>>,
    ) -> Vec<Wire> {
        let mut bridges = Vec::new();
        for (&node, pins) in removed {
            let Some((input, output)) = *pins else {
                continue;
            };
            let sources =
                self.surviving_sources(InPin { node, input }, removed, &mut BTreeSet::new());
            let targets =
                self.surviving_targets(OutPin { node, output }, removed, &mut BTreeSet::new());
            for &from in &sources {
                for &to in &targets {
                    let wire = Wire { from, to };
                    if from.node != to.node
                        && !self.wires().contains(&wire)
                        && !bridges.contains(&wire)
                    {
                        bridges.push(wire);
                    }
                }
            }
        }
        bridges
    }

    /// The outputs feeding `pin`, looking through removed pass-through nodes.
    fn surviving_sources(
        &self,
        pin: InPin,
        removed: &BTreeMap<NodeId, Option<(InputId, OutputId)>>,
        visited: &mut BTreeSet<NodeId>,
    ) -> Vec<OutPin> {
        let mut sources = Vec::new();
        for from in self.sources_of(pin) {
            match removed.get(&from.node) {
                None => sources.push(from),
                Some(Some((input, output)))
                    if *output == from.output && visited.insert(from.node) =>
                {
                    sources.extend(self.surviving_sources(
                        InPin {
                            node: from.node,
                            input: *input,
                        },
                        removed,
                        visited,
                    ));
                }
                Some(_) => {}
            }
        }
        sources
    }

    /// The inputs fed by `pin`, looking through removed pass-through nodes.
    fn surviving_targets(
        &self,
        pin: OutPin,
        removed: &BTreeMap<NodeId, Option<(InputId, OutputId)>>,
        visited: &mut BTreeSet<NodeId>,
    ) -> Vec<InPin> {
        let mut targets = Vec::new();
        for to in self.targets_of(pin) {
            match removed.get(&to.node) {
                None => targets.push(to),
                Some(Some((input, output))) if *input == to.input && visited.insert(to.node) => {
                    targets.extend(self.surviving_targets(
                        OutPin {
                            node: to.node,
                            output: *output,
                        },
                        removed,
                        visited,
                    ));
                }
                Some(_) => {}
            }
        }
        targets
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ConnectionPolicy;
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

    fn wire(from: NodeId, to: NodeId) -> Wire {
        Wire {
            from: OutPin {
                node: from,
                output: OutputId(0),
            },
            to: InPin {
                node: to,
                input: InputId(0),
            },
        }
    }

    /// Every node passes through its pin 0 to its pin 0.
    fn pass_through(nodes: &[NodeId]) -> BTreeMap<NodeId, Option<(InputId, OutputId)>> {
        nodes
            .iter()
            .map(|node| (*node, Some((InputId(0), OutputId(0)))))
            .collect()
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

        assert_eq!(graph.downstream_nodes(a), vec![b, c]);
        assert!(graph.downstream_nodes(c).is_empty());
        assert_eq!(graph.downstream_nodes(unrelated), vec![c]);
    }

    #[test]
    fn upstream_follows_wires_backward_only() {
        let mut graph = Graph::new();
        let a = graph.add_node((), pos2(0.0, 0.0));
        let b = graph.add_node((), pos2(0.0, 0.0));
        let c = graph.add_node((), pos2(0.0, 0.0));
        let unrelated = graph.add_node((), pos2(0.0, 0.0));
        link(&mut graph, a, b);
        link(&mut graph, b, c);
        link(&mut graph, a, unrelated);

        assert_eq!(graph.upstream_nodes(c), vec![a, b]);
        assert!(graph.upstream_nodes(a).is_empty());
        assert_eq!(graph.upstream_nodes(unrelated), vec![a]);
    }

    #[test]
    fn downstream_terminates_on_a_cycle() {
        let mut graph = Graph::new();
        let a = graph.add_node((), pos2(0.0, 0.0));
        let b = graph.add_node((), pos2(0.0, 0.0));
        link(&mut graph, a, b);
        link(&mut graph, b, a);

        assert_eq!(
            graph.downstream_nodes(a),
            vec![a, b],
            "a reaches itself around the loop"
        );
    }

    #[test]
    fn removing_the_middle_of_a_chain_bridges_its_ends() {
        let mut graph = Graph::new();
        let a = graph.add_node((), pos2(0.0, 0.0));
        let b = graph.add_node((), pos2(0.0, 0.0));
        let c = graph.add_node((), pos2(0.0, 0.0));
        link(&mut graph, a, b);
        link(&mut graph, b, c);

        assert_eq!(graph.bridges_over(&pass_through(&[b])), vec![wire(a, c)]);
    }

    #[test]
    fn removing_several_chained_nodes_bridges_over_all_of_them() {
        let mut graph = Graph::new();
        let a = graph.add_node((), pos2(0.0, 0.0));
        let b = graph.add_node((), pos2(0.0, 0.0));
        let c = graph.add_node((), pos2(0.0, 0.0));
        let d = graph.add_node((), pos2(0.0, 0.0));
        link(&mut graph, a, b);
        link(&mut graph, b, c);
        link(&mut graph, c, d);

        assert_eq!(graph.bridges_over(&pass_through(&[b, c])), vec![wire(a, d)]);
    }

    #[test]
    fn a_removed_node_feeding_two_targets_bridges_to_both() {
        let mut graph = Graph::new();
        let a = graph.add_node((), pos2(0.0, 0.0));
        let b = graph.add_node((), pos2(0.0, 0.0));
        let c = graph.add_node((), pos2(0.0, 0.0));
        let d = graph.add_node((), pos2(0.0, 0.0));
        link(&mut graph, a, b);
        link(&mut graph, b, c);
        link(&mut graph, b, d);

        assert_eq!(
            graph.bridges_over(&pass_through(&[b])),
            vec![wire(a, c), wire(a, d)]
        );
    }

    #[test]
    fn a_node_without_pass_through_pins_breaks_the_chain() {
        let mut graph = Graph::new();
        let a = graph.add_node((), pos2(0.0, 0.0));
        let b = graph.add_node((), pos2(0.0, 0.0));
        let c = graph.add_node((), pos2(0.0, 0.0));
        link(&mut graph, a, b);
        link(&mut graph, b, c);

        assert!(
            graph
                .bridges_over(&[(b, None)].into_iter().collect())
                .is_empty()
        );
    }

    /// Only the wire into the pass-through input and out of the pass-through
    /// output are bridged; a wire into some other input of the removed node
    /// simply goes.
    #[test]
    fn only_the_pass_through_pins_are_bridged() {
        let mut graph = Graph::new();
        let a = graph.add_node((), pos2(0.0, 0.0));
        let side = graph.add_node((), pos2(0.0, 0.0));
        let b = graph.add_node((), pos2(0.0, 0.0));
        let c = graph.add_node((), pos2(0.0, 0.0));
        link(&mut graph, a, b);
        graph
            .connect(
                OutPin {
                    node: side,
                    output: OutputId(0),
                },
                InPin {
                    node: b,
                    input: InputId(1),
                },
                ConnectionPolicy::Multiple,
            )
            .unwrap();
        link(&mut graph, b, c);

        assert_eq!(graph.bridges_over(&pass_through(&[b])), vec![wire(a, c)]);
    }

    #[test]
    fn a_bridge_never_loops_a_node_into_itself_and_never_duplicates_a_wire() {
        let mut graph = Graph::new();
        let a = graph.add_node((), pos2(0.0, 0.0));
        let b = graph.add_node((), pos2(0.0, 0.0));
        link(&mut graph, a, b);
        link(&mut graph, b, a);
        assert!(graph.bridges_over(&pass_through(&[b])).is_empty());

        let mut graph = Graph::new();
        let a = graph.add_node((), pos2(0.0, 0.0));
        let b = graph.add_node((), pos2(0.0, 0.0));
        let c = graph.add_node((), pos2(0.0, 0.0));
        link(&mut graph, a, b);
        link(&mut graph, b, c);
        link(&mut graph, a, c);
        assert!(graph.bridges_over(&pass_through(&[b])).is_empty());
    }
}
