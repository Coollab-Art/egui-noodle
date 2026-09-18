use crate::{Graph, NodeId};
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
    /// The plain response to an event: move what moved, delete what was
    /// deleted. An application with undo builds commands from the events
    /// instead of calling this.
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
            CanvasEvent::ContextMenuRequested { .. } => {}
        }
    }
}
