use super::{InPin, InputId, NodeId, OutPin, OutputId, Wire};
use egui::Pos2;
use std::collections::BTreeMap;

/// The node graph. `N` is whatever the application stores per node; the graph
/// never looks inside it.
///
/// Every mutation returns what its inverse needs, and none of them panics on a
/// stale id, so an undo/redo layer can be built on top without the graph
/// knowing about it. See `post-mortems/1-Application Owns the Model.md` and
/// `2-Node and Wire Identity.md`.
pub struct Graph<N> {
    /// Keyed by a monotonic id, so iteration is creation order and stable
    /// across runs - the compiled output must not depend on hash order.
    nodes: BTreeMap<NodeId, Node<N>>,
    next_id: u64,
    /// Insertion order, for the same reason. Every lookup scans it; at the
    /// hundreds of wires a hand-authored graph holds that is microseconds.
    wires: Vec<Wire>,
    topology_revision: u64,
    layout_revision: u64,
}

/// What the graph stores for one node.
pub struct Node<N> {
    /// Graph space, the node's top-left corner.
    pub pos: Pos2,
    pub payload: N,
}

/// How many wires one input accepts. Declared per input by the application;
/// the graph enforces it on `connect`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConnectionPolicy {
    /// A new wire displaces whatever was plugged in before.
    Single,
    Multiple,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Connected {
    New {
        /// The wires a `Single` input dropped to make room.
        displaced: Vec<Wire>,
    },
    /// The wire was already there. Nothing changed.
    AlreadyExisted,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Inserted {
    Done {
        /// The wires the new node's input dropped under its policy.
        displaced: Vec<Wire>,
    },
    /// The wire to splice into was no longer there. Nothing changed.
    WireGone,
}

/// A wire's end names a node the graph does not hold - a stale id, typically.
#[derive(Debug, PartialEq, Eq)]
pub struct UnknownNode(pub NodeId);

/// Everything needed to put a removed node back exactly as it was, with
/// `add_node_with_id` and `connect`.
pub struct RemovedNode<N> {
    pub node: Node<N>,
    /// Every wire that touched the node, incoming and outgoing.
    pub wires: Vec<Wire>,
}

/// The node already exists. Nothing changed.
#[derive(Debug, PartialEq, Eq)]
pub struct IdInUse(pub NodeId);

impl<N> Default for Graph<N> {
    fn default() -> Self {
        Self {
            nodes: BTreeMap::new(),
            next_id: 0,
            wires: Vec::new(),
            topology_revision: 0,
            layout_revision: 0,
        }
    }
}

impl<N> Graph<N> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_node(&mut self, payload: N, pos: Pos2) -> NodeId {
        let id = NodeId(self.next_id);
        self.next_id += 1;
        self.nodes.insert(id, Node { pos, payload });
        self.topology_revision += 1;
        id
    }

    /// Restores a node under an id it held before - on undo, or on loading a
    /// document. Ids minted afterwards stay above it.
    pub fn add_node_with_id(&mut self, id: NodeId, payload: N, pos: Pos2) -> Result<(), IdInUse> {
        if self.nodes.contains_key(&id) {
            return Err(IdInUse(id));
        }
        self.nodes.insert(id, Node { pos, payload });
        self.next_id = self.next_id.max(id.0.saturating_add(1));
        self.topology_revision += 1;
        Ok(())
    }

    /// `None` when the node is not in the graph.
    pub fn remove_node(&mut self, id: NodeId) -> Option<RemovedNode<N>> {
        let node = self.nodes.remove(&id)?;
        let wires = self
            .wires
            .extract_if(.., |wire| wire.from.node == id || wire.to.node == id)
            .collect();
        self.topology_revision += 1;
        Some(RemovedNode { node, wires })
    }

    /// Returns the previous position, or `None` when the node is not in the graph.
    pub fn set_pos(&mut self, id: NodeId, pos: Pos2) -> Option<Pos2> {
        let node = self.nodes.get_mut(&id)?;
        let previous = std::mem::replace(&mut node.pos, pos);
        if previous != pos {
            self.layout_revision += 1;
        }
        Some(previous)
    }

    pub fn connect(
        &mut self,
        from: OutPin,
        to: InPin,
        policy: ConnectionPolicy,
    ) -> Result<Connected, UnknownNode> {
        for node in [from.node, to.node] {
            if !self.nodes.contains_key(&node) {
                return Err(UnknownNode(node));
            }
        }
        let wire = Wire { from, to };
        if self.wires.contains(&wire) {
            return Ok(Connected::AlreadyExisted);
        }
        let displaced = match policy {
            ConnectionPolicy::Single => self
                .wires
                .extract_if(.., |existing| existing.to == to)
                .collect(),
            ConnectionPolicy::Multiple => Vec::new(),
        };
        self.wires.push(wire);
        self.topology_revision += 1;
        Ok(Connected::New { displaced })
    }

    /// Whether the wire was there to remove.
    pub fn disconnect(&mut self, wire: Wire) -> bool {
        let Some(index) = self.wires.iter().position(|existing| *existing == wire) else {
            return false;
        };
        self.wires.remove(index);
        self.topology_revision += 1;
        true
    }

    /// Splices `node` into `wire`: the wire's source now feeds `input`, and
    /// `output` now feeds the wire's old target.
    pub fn insert_into_wire(
        &mut self,
        wire: Wire,
        node: NodeId,
        input: InputId,
        input_policy: ConnectionPolicy,
        output: OutputId,
    ) -> Result<Inserted, UnknownNode> {
        if !self.nodes.contains_key(&node) {
            return Err(UnknownNode(node));
        }
        if !self.disconnect(wire) {
            return Ok(Inserted::WireGone);
        }
        let displaced = match self.connect(wire.from, InPin { node, input }, input_policy)? {
            Connected::New { displaced } => displaced,
            Connected::AlreadyExisted => Vec::new(),
        };
        // The old wire into the target is gone, so nothing is left to displace
        // there whatever its policy.
        self.connect(OutPin { node, output }, wire.to, ConnectionPolicy::Multiple)?;
        Ok(Inserted::Done { displaced })
    }

    pub fn node(&self, id: NodeId) -> Option<&Node<N>> {
        self.nodes.get(&id)
    }

    pub fn node_mut(&mut self, id: NodeId) -> Option<&mut Node<N>> {
        self.nodes.get_mut(&id)
    }

    pub fn contains(&self, id: NodeId) -> bool {
        self.nodes.contains_key(&id)
    }

    /// In creation order.
    pub fn nodes(&self) -> impl Iterator<Item = (NodeId, &Node<N>)> {
        self.nodes.iter().map(|(id, node)| (*id, node))
    }

    /// In connection order.
    pub fn wires(&self) -> &[Wire] {
        &self.wires
    }

    /// The outputs wired into this input.
    pub fn sources_of(&self, pin: InPin) -> impl Iterator<Item = OutPin> + '_ {
        self.wires
            .iter()
            .filter(move |wire| wire.to == pin)
            .map(|wire| wire.from)
    }

    /// The inputs this output is wired into.
    pub fn targets_of(&self, pin: OutPin) -> impl Iterator<Item = InPin> + '_ {
        self.wires
            .iter()
            .filter(move |wire| wire.from == pin)
            .map(|wire| wire.to)
    }

    /// Bumped by every change to which nodes exist and how they are wired.
    /// Moving a node does not touch it, so a consumer that recompiles on
    /// topology changes is not woken by a drag.
    pub fn topology_revision(&self) -> u64 {
        self.topology_revision
    }

    /// Bumped by every node move.
    pub fn layout_revision(&self) -> u64 {
        self.layout_revision
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::pos2;
    use std::collections::HashSet;

    fn in_pin(node: NodeId, input: u64) -> InPin {
        InPin {
            node,
            input: InputId(input),
        }
    }

    fn out_pin(node: NodeId, output: u64) -> OutPin {
        OutPin {
            node,
            output: OutputId(output),
        }
    }

    fn wire(from: OutPin, to: InPin) -> Wire {
        Wire { from, to }
    }

    fn wire_set(wires: &[Wire]) -> HashSet<Wire> {
        wires.iter().copied().collect()
    }

    #[test]
    fn a_removed_node_id_is_never_handed_out_again() {
        let mut graph = Graph::new();
        let first = graph.add_node("a", pos2(0.0, 0.0));
        graph.remove_node(first);
        let second = graph.add_node("b", pos2(0.0, 0.0));

        assert_ne!(first, second);
        assert!(graph.node(first).is_none());
        assert_eq!(graph.node(second).map(|node| node.payload), Some("b"));
    }

    #[test]
    fn restoring_an_id_keeps_later_ids_above_it() {
        let mut graph = Graph::new();
        assert_eq!(
            graph.add_node_with_id(NodeId(100), "restored", pos2(0.0, 0.0)),
            Ok(())
        );
        let minted = graph.add_node("fresh", pos2(0.0, 0.0));
        assert!(minted > NodeId(100));
        assert_eq!(
            graph.add_node_with_id(NodeId(100), "again", pos2(0.0, 0.0)),
            Err(IdInUse(NodeId(100)))
        );
    }

    #[test]
    fn a_single_input_displaces_its_previous_wire() {
        let mut graph = Graph::new();
        let x = graph.add_node("x", pos2(0.0, 0.0));
        let y = graph.add_node("y", pos2(0.0, 0.0));
        let target = graph.add_node("target", pos2(0.0, 0.0));
        let input = in_pin(target, 0);

        graph
            .connect(out_pin(x, 0), input, ConnectionPolicy::Single)
            .unwrap();
        let outcome = graph
            .connect(out_pin(y, 0), input, ConnectionPolicy::Single)
            .unwrap();

        assert_eq!(
            outcome,
            Connected::New {
                displaced: vec![wire(out_pin(x, 0), input)]
            }
        );
        assert_eq!(
            graph.sources_of(input).collect::<Vec<_>>(),
            vec![out_pin(y, 0)]
        );
    }

    #[test]
    fn a_multiple_input_keeps_every_wire() {
        let mut graph = Graph::new();
        let x = graph.add_node("x", pos2(0.0, 0.0));
        let y = graph.add_node("y", pos2(0.0, 0.0));
        let target = graph.add_node("target", pos2(0.0, 0.0));
        let input = in_pin(target, 0);

        graph
            .connect(out_pin(x, 0), input, ConnectionPolicy::Multiple)
            .unwrap();
        let outcome = graph
            .connect(out_pin(y, 0), input, ConnectionPolicy::Multiple)
            .unwrap();

        assert_eq!(outcome, Connected::New { displaced: vec![] });
        assert_eq!(graph.sources_of(input).count(), 2);
    }

    #[test]
    fn connecting_the_same_wire_twice_changes_nothing() {
        let mut graph = Graph::new();
        let a = graph.add_node("a", pos2(0.0, 0.0));
        let b = graph.add_node("b", pos2(0.0, 0.0));

        graph
            .connect(out_pin(a, 0), in_pin(b, 0), ConnectionPolicy::Single)
            .unwrap();
        let revision = graph.topology_revision();
        let outcome = graph
            .connect(out_pin(a, 0), in_pin(b, 0), ConnectionPolicy::Single)
            .unwrap();

        assert_eq!(outcome, Connected::AlreadyExisted);
        assert_eq!(graph.wires().len(), 1);
        assert_eq!(graph.topology_revision(), revision);
    }

    #[test]
    fn connecting_to_a_missing_node_is_an_error() {
        let mut graph = Graph::new();
        let a = graph.add_node("a", pos2(0.0, 0.0));
        let gone = graph.add_node("gone", pos2(0.0, 0.0));
        graph.remove_node(gone);

        assert_eq!(
            graph.connect(out_pin(a, 0), in_pin(gone, 0), ConnectionPolicy::Single),
            Err(UnknownNode(gone))
        );
        assert!(graph.wires().is_empty());
    }

    /// The contract undo relies on: what `remove_node` hands back, fed to
    /// `add_node_with_id` and `connect`, rebuilds the graph exactly.
    #[test]
    fn a_removed_node_can_be_put_back_exactly() {
        let mut graph = Graph::new();
        let upstream = graph.add_node("upstream", pos2(0.0, 0.0));
        let middle = graph.add_node("middle", pos2(10.0, 20.0));
        let downstream = graph.add_node("downstream", pos2(0.0, 0.0));
        graph
            .connect(
                out_pin(upstream, 0),
                in_pin(middle, 0),
                ConnectionPolicy::Single,
            )
            .unwrap();
        graph
            .connect(
                out_pin(middle, 0),
                in_pin(downstream, 0),
                ConnectionPolicy::Single,
            )
            .unwrap();
        graph
            .connect(
                out_pin(middle, 1),
                in_pin(downstream, 1),
                ConnectionPolicy::Single,
            )
            .unwrap();
        let before = wire_set(graph.wires());

        let removed = graph.remove_node(middle).unwrap();
        assert_eq!(removed.node.payload, "middle");
        assert_eq!(removed.node.pos, pos2(10.0, 20.0));
        assert_eq!(removed.wires.len(), 3, "incoming and outgoing wires alike");
        assert!(graph.wires().is_empty());
        assert!(graph.remove_node(middle).is_none(), "already gone");

        graph
            .add_node_with_id(middle, removed.node.payload, removed.node.pos)
            .unwrap();
        for wire in removed.wires {
            graph
                .connect(wire.from, wire.to, ConnectionPolicy::Multiple)
                .unwrap();
        }
        assert_eq!(wire_set(graph.wires()), before);
    }

    #[test]
    fn disconnecting_reports_whether_the_wire_existed() {
        let mut graph = Graph::new();
        let a = graph.add_node("a", pos2(0.0, 0.0));
        let b = graph.add_node("b", pos2(0.0, 0.0));
        let ab = wire(out_pin(a, 0), in_pin(b, 0));
        graph
            .connect(ab.from, ab.to, ConnectionPolicy::Single)
            .unwrap();

        assert!(graph.disconnect(ab));
        assert!(graph.wires().is_empty());
        assert!(!graph.disconnect(ab));
    }

    /// Why there are two revisions: a drag must not look like an edit to a
    /// consumer that recompiles on topology changes.
    #[test]
    fn moving_a_node_bumps_layout_but_not_topology() {
        let mut graph = Graph::new();
        let a = graph.add_node("a", pos2(0.0, 0.0));
        let b = graph.add_node("b", pos2(0.0, 0.0));
        let (topology, layout) = (graph.topology_revision(), graph.layout_revision());

        assert_eq!(graph.set_pos(a, pos2(5.0, 5.0)), Some(pos2(0.0, 0.0)));
        assert_eq!(graph.topology_revision(), topology);
        assert_ne!(graph.layout_revision(), layout);

        let layout = graph.layout_revision();
        graph
            .connect(out_pin(a, 0), in_pin(b, 0), ConnectionPolicy::Single)
            .unwrap();
        assert_ne!(graph.topology_revision(), topology);
        assert_eq!(graph.layout_revision(), layout);
    }

    #[test]
    fn a_node_can_be_spliced_into_a_wire() {
        let mut graph = Graph::new();
        let a = graph.add_node("a", pos2(0.0, 0.0));
        let c = graph.add_node("c", pos2(0.0, 0.0));
        let ac = wire(out_pin(a, 0), in_pin(c, 0));
        graph
            .connect(ac.from, ac.to, ConnectionPolicy::Single)
            .unwrap();
        let b = graph.add_node("b", pos2(0.0, 0.0));

        let outcome = graph
            .insert_into_wire(ac, b, InputId(7), ConnectionPolicy::Single, OutputId(3))
            .unwrap();

        assert_eq!(outcome, Inserted::Done { displaced: vec![] });
        assert_eq!(
            wire_set(graph.wires()),
            wire_set(&[
                wire(out_pin(a, 0), in_pin(b, 7)),
                wire(out_pin(b, 3), in_pin(c, 0)),
            ])
        );
    }

    #[test]
    fn splicing_into_a_wire_that_no_longer_exists_changes_nothing() {
        let mut graph = Graph::new();
        let a = graph.add_node("a", pos2(0.0, 0.0));
        let b = graph.add_node("b", pos2(0.0, 0.0));
        let c = graph.add_node("c", pos2(0.0, 0.0));
        let never_connected = wire(out_pin(a, 0), in_pin(c, 0));

        let outcome = graph.insert_into_wire(
            never_connected,
            b,
            InputId(0),
            ConnectionPolicy::Single,
            OutputId(0),
        );

        assert_eq!(outcome, Ok(Inserted::WireGone));
        assert!(graph.wires().is_empty(), "no half-applied splice");
    }
}
