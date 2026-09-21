use crate::{Graph, NodeIdentifier, Wire};
use std::collections::BTreeSet;

/// What is selected: nodes and wires alike, so one Delete removes both.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selection<I> {
    pub nodes: BTreeSet<I>,
    pub wires: BTreeSet<Wire<I>>,
}

// By hand: a derive would demand `I: Default`, which an id has no reason to be.
impl<I> Default for Selection<I> {
    fn default() -> Self {
        Self {
            nodes: BTreeSet::new(),
            wires: BTreeSet::new(),
        }
    }
}

impl<I: NodeIdentifier> Selection<I> {
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty() && self.wires.is_empty()
    }

    pub fn clear(&mut self) {
        self.nodes.clear();
        self.wires.clear();
    }

    pub fn contains_node(&self, id: I) -> bool {
        self.nodes.contains(&id)
    }

    pub fn contains_wire(&self, wire: Wire<I>) -> bool {
        self.wires.contains(&wire)
    }

    pub fn toggle_node(&mut self, id: I) {
        if !self.nodes.remove(&id) {
            self.nodes.insert(id);
        }
    }

    pub fn toggle_wire(&mut self, wire: Wire<I>) {
        if !self.wires.remove(&wire) {
            self.wires.insert(wire);
        }
    }

    pub fn extend(&mut self, other: &Selection<I>) {
        self.nodes.extend(other.nodes.iter().copied());
        self.wires.extend(other.wires.iter().copied());
    }

    /// Drops what the graph no longer holds.
    pub(super) fn retain_present<N>(&mut self, graph: &Graph<N, I>) {
        self.nodes.retain(|id| graph.contains(*id));
        self.wires.retain(|wire| graph.wires().contains(wire));
    }
}
