use crate::{AnyPin, ConnectionPolicy, Graph, InPin, NodeId, OutPin, Wire};
use egui::Pos2;

/// What the user did on the canvas this frame. The canvas never changes the
/// graph itself: the application applies these - directly with
/// [`Graph::apply`], or by turning each into a command it can undo.
#[derive(Clone, Debug, PartialEq)]
pub enum CanvasEvent {
    /// A drag ended. One event per drag, so one undo entry per drag.
    NodesMoved { moves: Vec<NodeMove> },
    /// The delete key, with these nodes selected.
    DeleteRequested { nodes: Vec<NodeId> },
    /// A wire was dropped on a pin. `policy` is the input's, as the
    /// application declared it.
    ConnectRequested {
        from: OutPin,
        to: InPin,
        policy: ConnectionPolicy,
    },
    /// A wire was picked up off its input and dropped elsewhere, or
    /// right-clicked.
    DisconnectRequested { wire: Wire },
    /// A new wire, started on `from`, was released over empty canvas at this
    /// graph-space position - the application may offer a node to complete it.
    WireDropped { from: AnyPin, pos: Pos2 },
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
    /// connected, delete what was deleted. An application with undo builds
    /// commands from the events instead of calling this.
    pub fn apply(&mut self, event: &CanvasEvent) {
        match event {
            CanvasEvent::NodesMoved { moves } => {
                for node_move in moves {
                    self.set_pos(node_move.id, node_move.to);
                }
            }
            CanvasEvent::DeleteRequested { nodes } => {
                for id in nodes {
                    self.remove_node(*id);
                }
            }
            CanvasEvent::ConnectRequested { from, to, policy } => {
                // A stale end is the one way this fails, and there is nothing
                // to do about it here: the wire simply is not made.
                let _ = self.connect(*from, *to, *policy);
            }
            CanvasEvent::DisconnectRequested { wire } => {
                self.disconnect(*wire);
            }
            CanvasEvent::WireDropped { .. } | CanvasEvent::ContextMenuRequested { .. } => {}
        }
    }
}
