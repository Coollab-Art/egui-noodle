use crate::{Graph, NodeId, Wire};
use std::collections::BTreeSet;

/// What is selected: nodes and wires alike, so one Delete removes both.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Selection {
    pub nodes: BTreeSet<NodeId>,
    pub wires: BTreeSet<Wire>,
}

impl Selection {
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty() && self.wires.is_empty()
    }

    pub fn clear(&mut self) {
        self.nodes.clear();
        self.wires.clear();
    }

    pub fn contains_node(&self, id: NodeId) -> bool {
        self.nodes.contains(&id)
    }

    pub fn contains_wire(&self, wire: Wire) -> bool {
        self.wires.contains(&wire)
    }

    pub fn toggle_node(&mut self, id: NodeId) {
        if !self.nodes.remove(&id) {
            self.nodes.insert(id);
        }
    }

    pub fn toggle_wire(&mut self, wire: Wire) {
        if !self.wires.remove(&wire) {
            self.wires.insert(wire);
        }
    }

    pub fn extend(&mut self, other: &Selection) {
        self.nodes.extend(other.nodes.iter().copied());
        self.wires.extend(other.wires.iter().copied());
    }

    /// Drops what the graph no longer holds.
    pub(super) fn retain_present<N>(&mut self, graph: &Graph<N>) {
        self.nodes.retain(|id| graph.contains(*id));
        self.wires.retain(|wire| graph.wires().contains(wire));
    }
}
