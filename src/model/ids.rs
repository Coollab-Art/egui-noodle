use std::fmt::Debug;
use std::hash::Hash;

/// What identifies a node: the application's type, never the graph's. The
/// graph stores and compares ids and never mints or interprets one - which
/// identity scheme a document needs (a session counter, a peer-unique pair, a
/// UUID) is the application's decision, so `Graph<N, I>` takes it as a
/// parameter and offers no default. See `2-Node and Wire Identity.md` and
/// `6-Application Owns Identity.md`.
///
/// Whatever the scheme, an id must **never be recycled**: an id held across
/// its node's deletion must never address a different node later. That is
/// what makes ids safe to serialize and to hold in an undo stack.
///
/// Implemented for every `Copy + Ord + Hash + Debug` type.
pub trait NodeIdentifier: Copy + Ord + Hash + Debug {}

impl<T: Copy + Ord + Hash + Debug> NodeIdentifier for T {}

/// A ready-made id for applications that do not care about the scheme: mint
/// `SequentialNodeId(0)`, `SequentialNodeId(1)`, ... and never reuse one. The
/// demo uses it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SequentialNodeId(pub u64);

/// Identifies one input pin of a node. Supplied by the application and never
/// interpreted by the graph, which only stores and compares it - so it can be a
/// stable hash of a name, a minted counter, or whatever the application's
/// pins are actually identified by. The display label is a separate concern.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct InputId(pub u64);

/// Identifies one output pin of a node. See [`InputId`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct OutputId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct InPin<I> {
    pub node: I,
    pub input: InputId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct OutPin<I> {
    pub node: I,
    pub output: OutputId,
}

/// Either end of a wire.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum AnyPin<I> {
    In(InPin<I>),
    Out(OutPin<I>),
}

impl<I: NodeIdentifier> AnyPin<I> {
    pub fn node(self) -> I {
        match self {
            AnyPin::In(pin) => pin.node,
            AnyPin::Out(pin) => pin.node,
        }
    }
}

/// A connection from an output pin to an input pin. A wire has no identity
/// beyond its two ends: this pair *is* the wire.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Wire<I> {
    pub from: OutPin<I>,
    pub to: InPin<I>,
}
