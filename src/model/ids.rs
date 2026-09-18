/// Identifies a node. Minted by the graph, monotonically, and **never
/// recycled**: an id held across its node's deletion can never address a
/// different node later. Safe to serialize and to hold in an undo stack.
///
/// The inner value is public so an application can restore ids from a saved
/// document (`Graph::add_node_with_id`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId(pub u64);

/// Identifies one input pin of a node. Supplied by the application and never
/// interpreted by the graph, which only stores and compares it - so it can be a
/// stable hash of a name, a minted counter, or whatever the application's
/// pins are actually identified by. The display label is a separate concern.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct InputId(pub u64);

/// Identifies one output pin of a node. See [`InputId`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct OutputId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct InPin {
    pub node: NodeId,
    pub input: InputId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct OutPin {
    pub node: NodeId,
    pub output: OutputId,
}

/// Either end of a wire.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum AnyPin {
    In(InPin),
    Out(OutPin),
}

impl AnyPin {
    pub fn node(self) -> NodeId {
        match self {
            AnyPin::In(pin) => pin.node,
            AnyPin::Out(pin) => pin.node,
        }
    }
}

/// A connection from an output pin to an input pin. A wire has no identity
/// beyond its two ends: this pair *is* the wire.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Wire {
    pub from: OutPin,
    pub to: InPin,
}
