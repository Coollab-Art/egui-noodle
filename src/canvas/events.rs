use crate::{AnyPin, ConnectionPolicy, Graph, InPin, InputId, NodeId, OutPin, OutputId, Wire};
use egui::Pos2;

/// What the user did on the canvas this frame. The canvas never changes the
/// graph itself: the application applies these - directly with
/// [`Graph::apply`], or by turning each into a command it can undo. See
/// `1-Application Owns the Model.md`.
#[derive(Clone, Debug, PartialEq)]
pub enum CanvasEvent {
    /// A drag ended, or nodes finished sliding apart to make room. One event
    /// per drag, so one undo entry per drag.
    NodesMoved { moves: Vec<NodeMove> },
    /// The delete key with these selected, or a cut stroke through them.
    /// `wires` never touches a node in `nodes` - those go with the node.
    /// `bridges` are the wires that close the chains the removed nodes sat
    /// in (`A -> B -> C` less `B` is `A -> C`), computed by
    /// [`Graph::bridges_over`] from each node's `splice_pins`. Why the event
    /// carries them: `1-Application Owns the Model.md`.
    DeleteRequested {
        nodes: Vec<NodeId>,
        wires: Vec<Wire>,
        bridges: Vec<Wire>,
    },
    /// A wire was dropped on a pin. `policy` is the input's, as the
    /// application declared it.
    ConnectRequested {
        from: OutPin,
        to: InPin,
        policy: ConnectionPolicy,
    },
    /// A new wire, started on `from`, was released over empty canvas at this
    /// graph-space position - the application may offer a node to complete it.
    WireDropped { from: AnyPin, pos: Pos2 },
    /// The `+` on a hovered wire was clicked, or the wire double-clicked -
    /// the application may offer a node to splice in, placed around `pos`.
    WireInsertRequested { wire: Wire, pos: Pos2 },
    /// A node was dropped on a wire: splice it in, `input` fed by the wire's
    /// source and `output` feeding the wire's target. Follows the
    /// `NodesMoved` of the same drop.
    NodeDroppedOnWire {
        wire: Wire,
        node: NodeId,
        input: InputId,
        input_policy: ConnectionPolicy,
        output: OutputId,
    },
    /// A secondary click on empty canvas, at this graph-space position. What
    /// to show is the application's business.
    ContextMenuRequested { pos: Pos2 },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NodeMove {
    pub id: NodeId,
    pub from: Pos2,
    pub to: Pos2,
}

impl<N> Graph<N> {
    /// The plain response to an event: move what moved, connect what was
    /// connected, delete what was deleted, splice what was dropped on a wire.
    /// An application with undo builds commands from the events instead of
    /// calling this.
    pub fn apply(&mut self, event: &CanvasEvent) {
        match event {
            CanvasEvent::NodesMoved { moves } => {
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
                    let _ = self.connect(bridge.from, bridge.to, ConnectionPolicy::Multiple);
                }
            }
            CanvasEvent::ConnectRequested { from, to, policy } => {
                // A stale end is the one way this fails, and there is nothing
                // to do about it here: the wire simply is not made.
                let _ = self.connect(*from, *to, *policy);
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
            | CanvasEvent::ContextMenuRequested { .. } => {}
        }
    }
}
