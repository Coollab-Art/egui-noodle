use super::Selection;
use crate::{
    AnyPin, ConnectionPolicy, Graph, InPin, InputId, NodeIdentifier, OutPin, OutputId, Wire,
};
use egui::Pos2;

/// What the user did on the canvas this frame. The canvas never changes the
/// graph itself: the application applies these - directly with
/// [`Graph::apply`], or by turning each into a command it can undo. See
/// `1-Application Owns the Model.md`.
#[derive(Clone, Debug, PartialEq)]
pub enum CanvasEvent<I> {
    /// Nodes are being dragged, or were just dropped. Reported **every frame
    /// the pointer moves**, not once per drag: a plugin reacting to a move, a
    /// peer mirroring the drag, or a recording of the session all need the
    /// intermediate positions, and an application that only wants the drop
    /// waits for `MovePhase::Finished`. Every dragged node is listed each
    /// time, whether or not it moved this frame, so the set is the same across
    /// the drag - what an application grouping the drag into one undo entry
    /// keys on. Why every frame: `6-Application Owns Identity.md`.
    NodesMoved {
        moves: Vec<NodeMove<I>>,
        phase: MovePhase,
    },
    /// The delete key with these selected, or a cut stroke through them.
    /// `wires` never touches a node in `nodes` - those go with the node.
    /// `bridges` are the wires that close the chains the removed nodes sat
    /// in (`A -> B -> C` less `B` is `A -> C`), computed by
    /// [`Graph::bridges_over`] from each node's `splice_pins`. Why the event
    /// carries them: `1-Application Owns the Model.md`.
    DeleteRequested {
        nodes: Vec<I>,
        wires: Vec<Wire<I>>,
        bridges: Vec<Wire<I>>,
    },
    /// A wire was dropped on a pin. `policy` is the input's, as the
    /// application declared it.
    ConnectRequested {
        from: OutPin<I>,
        to: InPin<I>,
        policy: ConnectionPolicy,
    },
    /// A new wire, started on `from`, was released over empty canvas at this
    /// graph-space position - the application may offer a node to complete it.
    WireDropped { from: AnyPin<I>, pos: Pos2 },
    /// The `+` on a hovered wire was clicked, or the wire double-clicked -
    /// the application may offer a node to splice in, placed around `pos`.
    WireInsertRequested { wire: Wire<I>, pos: Pos2 },
    /// A node was dropped on a wire: splice it in, `input` fed by the wire's
    /// source and `output` feeding the wire's target. Follows the
    /// `NodesMoved` of the same drop.
    NodeDroppedOnWire {
        wire: Wire<I>,
        node: I,
        input: InputId,
        input_policy: ConnectionPolicy,
        output: OutputId,
    },
    /// A secondary click on empty canvas, at this graph-space position. What
    /// to show is the application's business.
    ContextMenuRequested { pos: Pos2 },
    /// The selection is not what it was at the start of the frame. Selection
    /// is view state, so nothing needs applying; this is for an application
    /// that mirrors it - a properties panel, a script, a peer.
    SelectionChanged { selection: Selection<I> },
}

#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct NodeMove<I> {
    pub id: I,
    /// Graph space, the node's new top-left corner.
    pub to: Pos2,
}

/// Where in a drag a `NodesMoved` was reported.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum MovePhase {
    /// The pointer is still down.
    Ongoing,
    /// The pointer was released. The last event of the drag, and the one an
    /// application that groups a drag into one undo entry closes it on.
    Finished,
}

impl<N, I: NodeIdentifier> Graph<N, I> {
    /// The plain response to an event: move what moved, connect what was
    /// connected (displacing under `Single`), delete what was deleted, splice
    /// what was dropped on a wire. An application with undo builds commands
    /// from the events instead of calling this.
    pub fn apply(&mut self, event: &CanvasEvent<I>) {
        match event {
            CanvasEvent::NodesMoved { moves, .. } => {
                for node_move in moves {
                    self.set_pos(node_move.id, node_move.to);
                }
            }
            CanvasEvent::DeleteRequested {
                nodes,
                wires,
                bridges,
            } => {
                for id in nodes {
                    self.remove_node(*id);
                }
                for wire in wires {
                    self.disconnect(*wire);
                }
                // The removed node's wire into each bridged target is gone,
                // so there is nothing left to displace whatever the policy.
                for bridge in bridges {
                    let _ = self.connect(*bridge);
                }
            }
            CanvasEvent::ConnectRequested { from, to, policy } => {
                // A stale end is the one way this fails, and there is nothing
                // to do about it here: the wire simply is not made.
                if self.contains(from.node) && self.contains(to.node) {
                    self.displace(*to, *policy);
                    let _ = self.connect(Wire {
                        from: *from,
                        to: *to,
                    });
                }
            }
            CanvasEvent::NodeDroppedOnWire {
                wire,
                node,
                input,
                input_policy,
                output,
            } => {
                let _ = self.insert_into_wire(*wire, *node, *input, *input_policy, *output);
            }
            CanvasEvent::WireDropped { .. }
            | CanvasEvent::WireInsertRequested { .. }
            | CanvasEvent::ContextMenuRequested { .. }
            | CanvasEvent::SelectionChanged { .. } => {}
        }
    }
}
