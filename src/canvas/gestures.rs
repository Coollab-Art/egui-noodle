//! Selecting, dragging and deleting nodes, and box selection. The gesture in
//! progress lives on the canvas; every effect on the graph leaves as a
//! `CanvasEvent`.

use super::{Canvas, CanvasEvent, CanvasStyle, NodeMove, snap};
use crate::{Graph, NodeId};
use egui::{Key, Modifiers, PointerButton, Pos2, Rect, Response, Vec2};
use std::collections::BTreeMap;

#[derive(Default)]
pub(super) enum Gesture {
    #[default]
    Idle,
    DraggingNodes {
        /// The node whose frame the drag started on; its response says when
        /// the drag ends.
        handle: NodeId,
        /// Where each dragged node was when the drag began.
        start: BTreeMap<NodeId, Pos2>,
        /// Each dragged node's offset from the pointer, so the group keeps its
        /// shape and the grabbed point stays under the cursor.
        grab: BTreeMap<NodeId, Vec2>,
        /// Where the nodes are drawn this frame. The graph is only told on
        /// release.
        targets: BTreeMap<NodeId, Pos2>,
        guide_x: Option<f32>,
        guide_y: Option<f32>,
    },
    BoxSelecting {
        start: Pos2,
        current: Pos2,
    },
}

impl Canvas {
    /// Reads this frame's node-frame responses and the background's, updates
    /// the gesture and the selection, and reports what changed.
    pub(super) fn handle_input<N>(
        &mut self,
        graph: &Graph<N>,
        frames: &[(NodeId, Response)],
        background: &Response,
        pointer: Option<Pos2>,
        modifiers: Modifiers,
        style: &CanvasStyle,
        events: &mut Vec<CanvasEvent>,
    ) {
        self.selection.retain(|id| graph.contains(*id));

        for (id, response) in frames {
            if response.clicked() {
                if modifiers.command {
                    self.toggle_selected(*id);
                } else {
                    self.select_only(*id);
                }
                self.raise(*id);
            }
            if response.drag_started_by(PointerButton::Primary) {
                if !self.selection.contains(id) {
                    self.select_only(*id);
                }
                self.raise(*id);
                if let Some(pointer) = pointer {
                    self.gesture = self.begin_node_drag(graph, *id, pointer);
                }
            }
        }

        // Taken out for the duration, so the arms can borrow the rest of `self`.
        let mut gesture = std::mem::take(&mut self.gesture);
        let mut finished = false;
        match &mut gesture {
            Gesture::Idle => {}
            Gesture::DraggingNodes {
                handle,
                start,
                grab,
                targets,
                guide_x,
                guide_y,
            } => {
                if let Some(pointer) = pointer {
                    for (id, offset) in grab.iter() {
                        targets.insert(*id, pointer + *offset);
                    }
                    let snapped = if modifiers.shift {
                        snap::Snap {
                            delta: Vec2::ZERO,
                            guide_x: None,
                            guide_y: None,
                        }
                    } else {
                        self.snap_targets(targets, style)
                    };
                    for target in targets.values_mut() {
                        *target += snapped.delta;
                    }
                    *guide_x = snapped.guide_x;
                    *guide_y = snapped.guide_y;
                }

                let handle_response = frames
                    .iter()
                    .find(|(id, _)| id == handle)
                    .map(|(_, response)| response);
                let ended = handle_response.is_none_or(|response| response.drag_stopped());
                if ended {
                    let moves: Vec<NodeMove> = start
                        .iter()
                        .filter_map(|(id, from)| {
                            let to = *targets.get(id)?;
                            (to != *from).then_some(NodeMove {
                                id: *id,
                                from: *from,
                                to,
                            })
                        })
                        .collect();
                    if !moves.is_empty() {
                        events.push(CanvasEvent::NodesMoved { moves });
                    }
                    finished = true;
                }
            }
            Gesture::BoxSelecting { start, current } => {
                if let Some(pointer) = pointer {
                    *current = pointer;
                }
                if background.drag_stopped() {
                    let rect = Rect::from_two_pos(*start, *current);
                    let inside: Vec<NodeId> = self.layout.nodes_intersecting(rect).collect();
                    if !modifiers.shift {
                        self.selection.clear();
                    }
                    self.selection.extend(inside);
                    finished = true;
                }
            }
        }
        self.gesture = if finished { Gesture::Idle } else { gesture };

        if background.drag_started_by(PointerButton::Primary)
            && let Some(pointer) = pointer
        {
            self.gesture = Gesture::BoxSelecting {
                start: pointer,
                current: pointer,
            };
        }
        if background.clicked() && !modifiers.command {
            self.selection.clear();
        }
        if background.secondary_clicked()
            && let Some(pos) = pointer
        {
            events.push(CanvasEvent::ContextMenuRequested { pos });
        }

        let delete_pressed = background
            .ctx
            .input(|input| input.key_pressed(Key::Delete) || input.key_pressed(Key::Backspace));
        if delete_pressed
            && background.contains_pointer()
            && !background.ctx.egui_wants_keyboard_input()
            && !self.selection.is_empty()
        {
            events.push(CanvasEvent::DeleteRequested {
                nodes: self.selection.iter().copied().collect(),
            });
        }
    }

    fn begin_node_drag<N>(&self, graph: &Graph<N>, handle: NodeId, pointer: Pos2) -> Gesture {
        let start: BTreeMap<NodeId, Pos2> = self
            .selection
            .iter()
            .filter_map(|id| Some((*id, graph.node(*id)?.pos)))
            .collect();
        let grab = start
            .iter()
            .map(|(id, pos)| (*id, *pos - pointer))
            .collect();
        Gesture::DraggingNodes {
            handle,
            targets: start.clone(),
            start,
            grab,
            guide_x: None,
            guide_y: None,
        }
    }

    /// Snaps the dragged group as a whole, against the nodes not being
    /// dragged, using where everything was drawn last frame.
    fn snap_targets(&self, targets: &BTreeMap<NodeId, Pos2>, style: &CanvasStyle) -> snap::Snap {
        let group = targets
            .iter()
            .filter_map(|(id, pos)| {
                let size = self.layout.nodes.get(id)?.rect.size();
                Some(Rect::from_min_size(*pos, size))
            })
            .reduce(|a, b| a.union(b));
        let Some(mut group) = group else {
            return snap::Snap {
                delta: Vec2::ZERO,
                guide_x: None,
                guide_y: None,
            };
        };
        let mut delta = Vec2::ZERO;
        if style.snap_to_grid && style.grid_spacing > 0.0 {
            let snapped_min = (group.min / style.grid_spacing).round() * style.grid_spacing;
            delta = snapped_min - group.min;
            group = group.translate(delta);
        }
        let others = self
            .layout
            .nodes
            .iter()
            .filter(|(id, _)| !targets.contains_key(id))
            .map(|(_, node)| node.rect);
        let mut snapped = snap::snap_to_nodes(group, others, style.snap_distance / self.view.zoom);
        snapped.delta += delta;
        snapped
    }

    /// Where a node is drawn this frame: its provisional position while it is
    /// being dragged, its graph position otherwise.
    pub(super) fn drawn_pos(&self, id: NodeId, graph_pos: Pos2) -> Pos2 {
        match &self.gesture {
            Gesture::DraggingNodes { targets, .. } => {
                targets.get(&id).copied().unwrap_or(graph_pos)
            }
            Gesture::Idle | Gesture::BoxSelecting { .. } => graph_pos,
        }
    }

    pub(super) fn raise(&mut self, id: NodeId) {
        if let Some(index) = self.draw_order.iter().position(|other| *other == id) {
            self.draw_order.remove(index);
            self.draw_order.push(id);
        }
    }

    fn toggle_selected(&mut self, id: NodeId) {
        if !self.selection.remove(&id) {
            self.selection.insert(id);
        }
    }
}
