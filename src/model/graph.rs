use super::{InPin, InputId, NodeIdentifier, OutPin, OutputId, Wire};
use egui::Pos2;
use std::collections::BTreeMap;

/// The node graph. `N` is whatever the application stores per node and `I` is
/// how it identifies one; the graph never looks inside either, and never
/// mints an id - see [`NodeIdentifier`](super::NodeIdentifier).
///
/// The mutations are the primitives an undo layer wants: each does one thing,
/// each returns what its inverse needs, and none of them panics on a stale
/// id. Policy - what a wire dropped on an occupied input should do - lives
/// above them, in `Graph::apply` and `insert_into_wire`, so an application
/// with its own rules can build on the primitives directly. See
/// `1-Application Owns the Model.md`, `2-Node and Wire Identity.md` and
/// `6-Application Owns Identity.md`.
pub struct Graph<N, I: NodeIdentifier> {
    /// Keyed by id, so iteration order is stable across runs - the compiled
    /// output must not depend on hash order.
    nodes: BTreeMap<I, Node<N>>,
    /// Insertion order, for the same reason. Every lookup scans it; at the
    /// hundreds of wires a hand-authored graph holds that is microseconds.
    wires: Vec<Wire<I>>,
    topology_revision: u64,
    layout_revision: u64,
}

/// What the graph stores for one node.
pub struct Node<N> {
    /// Graph space, the node's top-left corner.
    pub pos: Pos2,
    pub payload: N,
}

/// How many wires one input accepts. Declared per input by the application,
/// carried on `ConnectRequested`, and enforced by `Graph::apply` and
/// `insert_into_wire` - never by `connect` itself, which just adds a wire.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConnectionPolicy {
    /// A new wire displaces whatever was plugged in before.
    Single,
    Multiple,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Connected {
    New,
    /// The wire was already there. Nothing changed.
    AlreadyExisted,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Inserted<I> {
    Done {
        /// The wires the new node's input dropped under its policy.
        displaced: Vec<Wire<I>>,
    },
    /// The wire to splice into was no longer there. Nothing changed.
    WireGone,
}

/// A wire's end names a node the graph does not hold - a stale id, typically.
#[derive(Debug, PartialEq, Eq)]
pub struct UnknownNode<I>(pub I);

/// Everything needed to put a removed node back exactly as it was, with
/// `add_node` and `connect`.
pub struct RemovedNode<N, I> {
    pub node: Node<N>,
    /// Every wire that touched the node, incoming and outgoing.
    pub wires: Vec<Wire<I>>,
}

/// The node already exists. Nothing changed.
#[derive(Debug, PartialEq, Eq)]
pub struct IdInUse<I>(pub I);

impl<N, I: NodeIdentifier> Default for Graph<N, I> {
    fn default() -> Self {
        Self {
            nodes: BTreeMap::new(),
            wires: Vec::new(),
            topology_revision: 0,
            layout_revision: 0,
        }
    }
}

impl<N, I: NodeIdentifier> Graph<N, I> {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a node under an id the application minted - a fresh one, or the
    /// one a node held before it was removed, on undo or on loading a
    /// document.
    pub fn add_node(&mut self, id: I, payload: N, pos: Pos2) -> Result<(), IdInUse<I>> {
        if self.nodes.contains_key(&id) {
            return Err(IdInUse(id));
        }
        self.nodes.insert(id, Node { pos, payload });
        self.topology_revision += 1;
        Ok(())
    }

    /// Removes the node and every wire touching it. `None` when the node is
    /// not in the graph.
    pub fn remove_node(&mut self, id: I) -> Option<RemovedNode<N, I>> {
        let node = self.nodes.remove(&id)?;
        let wires = self
            .wires
            .extract_if(.., |wire| wire.from.node == id || wire.to.node == id)
            .collect();
        self.topology_revision += 1;
        Some(RemovedNode { node, wires })
    }

    /// Returns the previous position, or `None` when the node is not in the graph.
    pub fn set_pos(&mut self, id: I, pos: Pos2) -> Option<Pos2> {
        let node = self.nodes.get_mut(&id)?;
        let previous = std::mem::replace(&mut node.pos, pos);
        if previous != pos {
            self.layout_revision += 1;
        }
        Some(previous)
    }

    /// Adds a wire. Just that: an input may hold any number of wires as far
    /// as the graph is concerned, so a `Single` input's previous wire is the
    /// caller's to remove first - see `Graph::apply`.
    pub fn connect(&mut self, wire: Wire<I>) -> Result<Connected, UnknownNode<I>> {
        for node in [wire.from.node, wire.to.node] {
            if !self.nodes.contains_key(&node) {
                return Err(UnknownNode(node));
            }
        }
        if self.wires.contains(&wire) {
            return Ok(Connected::AlreadyExisted);
        }
        self.wires.push(wire);
        self.topology_revision += 1;
        Ok(Connected::New)
    }

    /// Whether the wire was there to remove.
    pub fn disconnect(&mut self, wire: Wire<I>) -> bool {
        let Some(index) = self.wires.iter().position(|existing| *existing == wire) else {
            return false;
        };
        self.wires.remove(index);
        self.topology_revision += 1;
        true
    }

    /// Splices `node` into `wire`: the wire's source now feeds `input`, and
    /// `output` now feeds the wire's old target. A convenience over the
    /// primitives, for an application without undo; one with undo emits the
    /// disconnect and the two connects itself.
    pub fn insert_into_wire(
        &mut self,
        wire: Wire<I>,
        node: I,
        input: InputId,
        input_policy: ConnectionPolicy,
        output: OutputId,
    ) -> Result<Inserted<I>, UnknownNode<I>> {
        if !self.nodes.contains_key(&node) {
            return Err(UnknownNode(node));
        }
        if !self.disconnect(wire) {
            return Ok(Inserted::WireGone);
        }
        let into = InPin { node, input };
        let displaced = self.displace(into, input_policy);
        self.connect(Wire {
            from: wire.from,
            to: into,
        })?;
        // The old wire into the target is gone, so nothing is left to displace
        // there whatever its policy.
        self.connect(Wire {
            from: OutPin { node, output },
            to: wire.to,
        })?;
        Ok(Inserted::Done { displaced })
    }

    /// What the policy says must go before a new wire lands on `input`:
    /// under `Single`, every wire already there. Removes and returns them.
    pub fn displace(&mut self, input: InPin<I>, policy: ConnectionPolicy) -> Vec<Wire<I>> {
        match policy {
            ConnectionPolicy::Single => {
                let displaced: Vec<Wire<I>> = self
                    .wires
                    .extract_if(.., |existing| existing.to == input)
                    .collect();
                if !displaced.is_empty() {
                    self.topology_revision += 1;
                }
                displaced
            }
            ConnectionPolicy::Multiple => Vec::new(),
        }
    }

    pub fn node(&self, id: I) -> Option<&Node<N>> {
        self.nodes.get(&id)
    }

    pub fn node_mut(&mut self, id: I) -> Option<&mut Node<N>> {
        self.nodes.get_mut(&id)
    }

    pub fn contains(&self, id: I) -> bool {
        self.nodes.contains_key(&id)
    }

    /// In id order.
    pub fn nodes(&self) -> impl Iterator<Item = (I, &Node<N>)> {
        self.nodes.iter().map(|(id, node)| (*id, node))
    }

    /// In connection order.
    pub fn wires(&self) -> &[Wire<I>] {
        &self.wires
    }

    /// The outputs wired into this input.
    pub fn sources_of(&self, pin: InPin<I>) -> impl Iterator<Item = OutPin<I>> + '_ {
        self.wires
            .iter()
            .filter(move |wire| wire.to == pin)
            .map(|wire| wire.from)
    }

    /// The inputs this output is wired into.
    pub fn targets_of(&self, pin: OutPin<I>) -> impl Iterator<Item = InPin<I>> + '_ {
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
    use crate::SequentialNodeId;
    use egui::pos2;
    use std::collections::HashSet;

    type Id = SequentialNodeId;

    fn in_pin(node: Id, input: u64) -> InPin<Id> {
        InPin {
            node,
            input: InputId(input),
        }
    }

    fn out_pin(node: Id, output: u64) -> OutPin<Id> {
        OutPin {
            node,
            output: OutputId(output),
        }
    }

    fn wire(from: OutPin<Id>, to: InPin<Id>) -> Wire<Id> {
        Wire { from, to }
    }

    fn wire_set(wires: &[Wire<Id>]) -> HashSet<Wire<Id>> {
        wires.iter().copied().collect()
    }

    /// Adds a node under the next sequential id.
    fn add<N>(graph: &mut Graph<N, Id>, payload: N, pos: Pos2) -> Id {
        let id = SequentialNodeId(graph.nodes().count() as u64 + 1000);
        graph.add_node(id, payload, pos).unwrap();
        id
    }

    #[test]
    fn adding_a_node_under_an_existing_id_changes_nothing() {
        let mut graph = Graph::new();
        let id = SequentialNodeId(7);
        assert_eq!(graph.add_node(id, "a", pos2(0.0, 0.0)), Ok(()));
        assert_eq!(graph.add_node(id, "b", pos2(1.0, 1.0)), Err(IdInUse(id)));
        assert_eq!(graph.node(id).map(|node| node.payload), Some("a"));
    }

    #[test]
    fn connecting_just_adds_a_wire_whatever_is_already_on_the_input() {
        let mut graph = Graph::new();
        let x = add(&mut graph, "x", pos2(0.0, 0.0));
        let y = add(&mut graph, "y", pos2(0.0, 0.0));
        let target = add(&mut graph, "target", pos2(0.0, 0.0));
        let input = in_pin(target, 0);

        assert_eq!(
            graph.connect(wire(out_pin(x, 0), input)),
            Ok(Connected::New)
        );
        assert_eq!(
            graph.connect(wire(out_pin(y, 0), input)),
            Ok(Connected::New)
        );
        assert_eq!(graph.sources_of(input).count(), 2);
    }

    #[test]
    fn displacing_a_single_input_removes_and_returns_its_wires() {
        let mut graph = Graph::new();
        let x = add(&mut graph, "x", pos2(0.0, 0.0));
        let target = add(&mut graph, "target", pos2(0.0, 0.0));
        let input = in_pin(target, 0);
        graph.connect(wire(out_pin(x, 0), input)).unwrap();

        assert_eq!(
            graph.displace(input, ConnectionPolicy::Single),
            vec![wire(out_pin(x, 0), input)]
        );
        assert!(graph.wires().is_empty());
        assert!(graph.displace(input, ConnectionPolicy::Multiple).is_empty());
    }

    #[test]
    fn connecting_the_same_wire_twice_changes_nothing() {
        let mut graph = Graph::new();
        let a = add(&mut graph, "a", pos2(0.0, 0.0));
        let b = add(&mut graph, "b", pos2(0.0, 0.0));

        graph.connect(wire(out_pin(a, 0), in_pin(b, 0))).unwrap();
        let revision = graph.topology_revision();
        let outcome = graph.connect(wire(out_pin(a, 0), in_pin(b, 0))).unwrap();

        assert_eq!(outcome, Connected::AlreadyExisted);
        assert_eq!(graph.wires().len(), 1);
        assert_eq!(graph.topology_revision(), revision);
    }

    #[test]
    fn connecting_to_a_missing_node_is_an_error() {
        let mut graph = Graph::new();
        let a = add(&mut graph, "a", pos2(0.0, 0.0));
        let gone = add(&mut graph, "gone", pos2(0.0, 0.0));
        graph.remove_node(gone);

        assert_eq!(
            graph.connect(wire(out_pin(a, 0), in_pin(gone, 0))),
            Err(UnknownNode(gone))
        );
        assert!(graph.wires().is_empty());
    }

    /// The contract undo relies on: what `remove_node` hands back, fed to
    /// `add_node` and `connect`, rebuilds the graph exactly.
    #[test]
    fn a_removed_node_can_be_put_back_exactly() {
        let mut graph = Graph::new();
        let upstream = add(&mut graph, "upstream", pos2(0.0, 0.0));
        let middle = add(&mut graph, "middle", pos2(10.0, 20.0));
        let downstream = add(&mut graph, "downstream", pos2(0.0, 0.0));
        graph
            .connect(wire(out_pin(upstream, 0), in_pin(middle, 0)))
            .unwrap();
        graph
            .connect(wire(out_pin(middle, 0), in_pin(downstream, 0)))
            .unwrap();
        graph
            .connect(wire(out_pin(middle, 1), in_pin(downstream, 1)))
            .unwrap();
        let before = wire_set(graph.wires());

        let removed = graph.remove_node(middle).unwrap();
        assert_eq!(removed.node.payload, "middle");
        assert_eq!(removed.node.pos, pos2(10.0, 20.0));
        assert_eq!(removed.wires.len(), 3, "incoming and outgoing wires alike");
        assert!(graph.wires().is_empty());
        assert!(graph.remove_node(middle).is_none(), "already gone");

        graph
            .add_node(middle, removed.node.payload, removed.node.pos)
            .unwrap();
        for wire in removed.wires {
            graph.connect(wire).unwrap();
        }
        assert_eq!(wire_set(graph.wires()), before);
    }

    #[test]
    fn disconnecting_reports_whether_the_wire_existed() {
        let mut graph = Graph::new();
        let a = add(&mut graph, "a", pos2(0.0, 0.0));
        let b = add(&mut graph, "b", pos2(0.0, 0.0));
        let ab = wire(out_pin(a, 0), in_pin(b, 0));
        graph.connect(ab).unwrap();

        assert!(graph.disconnect(ab));
        assert!(graph.wires().is_empty());
        assert!(!graph.disconnect(ab));
    }

    /// Why there are two revisions: a drag must not look like an edit to a
    /// consumer that recompiles on topology changes.
    #[test]
    fn moving_a_node_bumps_layout_but_not_topology() {
        let mut graph = Graph::new();
        let a = add(&mut graph, "a", pos2(0.0, 0.0));
        let b = add(&mut graph, "b", pos2(0.0, 0.0));
        let (topology, layout) = (graph.topology_revision(), graph.layout_revision());

        assert_eq!(graph.set_pos(a, pos2(5.0, 5.0)), Some(pos2(0.0, 0.0)));
        assert_eq!(graph.topology_revision(), topology);
        assert_ne!(graph.layout_revision(), layout);

        let layout = graph.layout_revision();
        graph.connect(wire(out_pin(a, 0), in_pin(b, 0))).unwrap();
        assert_ne!(graph.topology_revision(), topology);
        assert_eq!(graph.layout_revision(), layout);
    }

    #[test]
    fn a_node_can_be_spliced_into_a_wire() {
        let mut graph = Graph::new();
        let a = add(&mut graph, "a", pos2(0.0, 0.0));
        let c = add(&mut graph, "c", pos2(0.0, 0.0));
        let ac = wire(out_pin(a, 0), in_pin(c, 0));
        graph.connect(ac).unwrap();
        let b = add(&mut graph, "b", pos2(0.0, 0.0));

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
        let a = add(&mut graph, "a", pos2(0.0, 0.0));
        let b = add(&mut graph, "b", pos2(0.0, 0.0));
        let c = add(&mut graph, "c", pos2(0.0, 0.0));
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
