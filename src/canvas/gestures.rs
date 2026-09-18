//! Selecting, dragging and deleting nodes, box selection, dragging wires,
//! cutting wires, and dropping a node onto a wire. The gesture in progress
//! lives on the canvas; every effect on the graph leaves as a `CanvasEvent`.

use super::{
    Canvas, CanvasEvent, CanvasStyle, NodeMove, WireLayout, polyline_crosses_segment, snap,
};
use crate::{AnyPin, Graph, InPin, NodeId, OutPin, Wire};
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
        /// The wire a single dragged node would be spliced into on release.
        insert_target: Option<InsertTarget>,
    },
    BoxSelecting {
        start: Pos2,
        current: Pos2,
    },
    DraggingWire {
        /// The pin the drag started on.
        origin: AnyPin,
        /// Dragging from an input that already had wires picks them up
        /// instead of starting a new one: these move as a bundle, their
        /// output ends fixed, their loose ends following the pointer.
        detached: Vec<Wire>,
        current: Pos2,
    },
    CuttingWires {
        /// The stroke so far, in graph space.
        stroke: Vec<Pos2>,
        /// Every wire the stroke has crossed. Shown as it grows, severed on
        /// release.
        crossed: Vec<Wire>,
    },
}

/// The wire under a dragged node, and whether the node can go into it.
#[derive(Clone, Copy, Debug)]
pub(super) struct InsertTarget {
    pub wire: Wire,
    /// False when the node has no pins to splice with: the wire is shown as
    /// refusing, and releasing does nothing.
    pub valid: bool,
}

/// What is under the pointer while nothing is being dragged.
#[derive(Clone, Copy, Debug, Default)]
pub struct Hovered {
    pub wire: Option<Wire>,
    pub pin: Option<AnyPin>,
}

impl Canvas {
    /// Reads this frame's responses - node frames, pins, the wire's `+` and
    /// the background - updates the gesture and the selection, and reports
    /// what changed.
    pub(super) fn handle_input<N>(
        &mut self,
        graph: &Graph<N>,
        frames: &[(NodeId, Response)],
        pins: &[(AnyPin, Response)],
        insert_button: Option<&(Rect, Response)>,
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

        for (pin, response) in pins {
            if response.drag_started_by(PointerButton::Primary)
                && let Some(pointer) = pointer
            {
                let detached = match pin {
                    AnyPin::In(input) => graph
                        .sources_of(*input)
                        .map(|from| Wire { from, to: *input })
                        .collect(),
                    AnyPin::Out(_) => Vec::new(),
                };
                self.gesture = Gesture::DraggingWire {
                    origin: *pin,
                    detached,
                    current: pointer,
                };
            }
        }

        if let Some((rect, response)) = insert_button
            && response.clicked()
            && let Some(wire) = self.hovered.wire
        {
            events.push(CanvasEvent::WireInsertRequested {
                wire,
                pos: rect.center(),
            });
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
                insert_target,
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
                    *insert_target = self.insert_target_under(pointer, targets, modifiers, style);
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
                    if let Some(InsertTarget { wire, valid: true }) = insert_target
                        && let Some((&node, _)) = targets.iter().next()
                        && let Some(node_layout) = self.layout.nodes.get(&node)
                        && let Some((input, output)) = node_layout.splice_pins
                        && let Some(input_layout) =
                            node_layout.inputs.iter().find(|pin| pin.id == input)
                    {
                        events.push(CanvasEvent::NodeDroppedOnWire {
                            wire: *wire,
                            node,
                            input,
                            input_policy: input_layout.policy,
                            output,
                        });
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
            Gesture::DraggingWire {
                origin,
                detached,
                current,
            } => {
                if let Some(pointer) = pointer {
                    *current = pointer;
                }
                let origin_response = pins
                    .iter()
                    .find(|(pin, _)| pin == origin)
                    .map(|(_, response)| response);
                let ended = origin_response.is_none_or(|response| response.drag_stopped());
                if ended {
                    let target = self.layout.pin_at(*current, style.pin_hit_expansion);
                    self.release_wire(*origin, detached, target, *current, events);
                    finished = true;
                }
            }
            Gesture::CuttingWires { stroke, crossed } => {
                if let Some(pointer) = pointer
                    && stroke.last() != Some(&pointer)
                {
                    stroke.push(pointer);
                    // Only the newest segment can cross anything new.
                    let newest = &stroke[stroke.len() - 2..];
                    for wire in wires_crossed_by(newest, &self.layout.wires) {
                        if !crossed.contains(&wire) {
                            crossed.push(wire);
                        }
                    }
                }
                if background.drag_stopped() {
                    for wire in crossed.iter() {
                        events.push(CanvasEvent::DisconnectRequested { wire: *wire });
                    }
                    finished = true;
                }
            }
        }
        self.gesture = if finished { Gesture::Idle } else { gesture };

        if background.drag_started_by(PointerButton::Primary)
            && let Some(pointer) = pointer
        {
            // Under the command modifier node frames only sense clicks, so this
            // drag reaches the background even when it began over a node.
            self.gesture = if modifiers.command {
                Gesture::CuttingWires {
                    stroke: vec![pointer],
                    crossed: Vec::new(),
                }
            } else {
                Gesture::BoxSelecting {
                    start: pointer,
                    current: pointer,
                }
            };
        }
        if background.clicked() && !modifiers.command {
            self.selection.clear();
        }
        if background.secondary_clicked()
            && let Some(pos) = pointer
        {
            match self.hovered.wire {
                Some(wire) => events.push(CanvasEvent::DisconnectRequested { wire }),
                None => events.push(CanvasEvent::ContextMenuRequested { pos }),
            }
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

    /// What the pointer is over while nothing is being dragged, from where
    /// everything was drawn last frame. A hovered wire stays hovered while the
    /// pointer is on its `+`, which sits on the wire but is wider than it.
    pub(super) fn update_hovered(
        &mut self,
        pointer: Option<Pos2>,
        over_canvas: bool,
        style: &CanvasStyle,
    ) {
        let previous_wire = self.hovered.wire;
        self.hovered = Hovered::default();
        if !matches!(self.gesture, Gesture::Idle) || !over_canvas {
            return;
        }
        let Some(pointer) = pointer else {
            return;
        };
        let on_previous_button = previous_wire.filter(|wire| {
            self.insert_button_rect(*wire, style)
                .is_some_and(|rect| rect.contains(pointer))
        });
        if on_previous_button.is_some() {
            self.hovered.wire = on_previous_button;
            return;
        }
        self.hovered.pin = self.layout.pin_at(pointer, style.pin_hit_expansion);
        if self.hovered.pin.is_none() && self.layout.node_at(pointer).is_none() {
            self.hovered.wire = self.layout.wire_at(pointer, style.wire_hit_slack);
        }
    }

    /// Where a wire's `+` was drawn last frame.
    pub(super) fn insert_button_rect(&self, wire: Wire, style: &CanvasStyle) -> Option<Rect> {
        let center = self
            .layout
            .wires
            .iter()
            .find(|drawn| drawn.wire == wire)?
            .insert_point()?;
        Some(Rect::from_center_size(
            center,
            Vec2::splat(style.insert_button_size),
        ))
    }

    /// The wire a dragged node is over, when exactly one node is dragged and
    /// the alt modifier is not held down - alt is "move past this wire".
    fn insert_target_under(
        &self,
        pointer: Pos2,
        targets: &BTreeMap<NodeId, Pos2>,
        modifiers: Modifiers,
        style: &CanvasStyle,
    ) -> Option<InsertTarget> {
        if modifiers.alt || targets.len() != 1 {
            return None;
        }
        let (&node, _) = targets.iter().next()?;
        let wire = self
            .layout
            .wire_at(pointer, style.wire_hit_slack)
            .filter(|wire| wire.from.node != node && wire.to.node != node)?;
        let valid = self
            .layout
            .nodes
            .get(&node)
            .is_some_and(|layout| layout.splice_pins.is_some());
        Some(InsertTarget { wire, valid })
    }

    /// What a wire drag becomes when it is released over `target`.
    ///
    /// Picked-up wires move to a compatible input, are dropped on empty
    /// canvas, and are left alone when released anywhere else. A new wire
    /// connects to a compatible pin, asks the application for a node when
    /// released on empty canvas, and otherwise does nothing.
    fn release_wire(
        &self,
        origin: AnyPin,
        detached: &[Wire],
        target: Option<AnyPin>,
        pos: Pos2,
        events: &mut Vec<CanvasEvent>,
    ) {
        if !detached.is_empty() {
            match target {
                Some(AnyPin::In(to)) if Some(AnyPin::In(to)) != Some(origin) => {
                    let Some(policy) = self.layout.input(to).map(|input| input.policy) else {
                        return;
                    };
                    for wire in detached {
                        if can_connect(wire.from, to) {
                            events.push(CanvasEvent::DisconnectRequested { wire: *wire });
                            events.push(CanvasEvent::ConnectRequested {
                                from: wire.from,
                                to,
                                policy,
                            });
                        }
                    }
                }
                None => {
                    for wire in detached {
                        events.push(CanvasEvent::DisconnectRequested { wire: *wire });
                    }
                }
                Some(_) => {}
            }
            return;
        }

        match (origin, target) {
            (AnyPin::Out(from), Some(AnyPin::In(to))) if can_connect(from, to) => {
                if let Some(policy) = self.layout.input(to).map(|input| input.policy) {
                    events.push(CanvasEvent::ConnectRequested { from, to, policy });
                }
            }
            (AnyPin::In(to), Some(AnyPin::Out(from))) if can_connect(from, to) => {
                if let Some(policy) = self.layout.input(to).map(|input| input.policy) {
                    events.push(CanvasEvent::ConnectRequested { from, to, policy });
                }
            }
            (from, None) => events.push(CanvasEvent::WireDropped { from, pos }),
            _ => {}
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
            insert_target: None,
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
    /// being dragged, its graph position plus any sliding offset otherwise.
    pub(super) fn drawn_pos(&self, id: NodeId, graph_pos: Pos2) -> Pos2 {
        if let Gesture::DraggingNodes { targets, .. } = &self.gesture
            && let Some(target) = targets.get(&id)
        {
            return *target;
        }
        graph_pos + self.sliding_offset(id)
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

/// The one rule the canvas itself imposes: a node does not wire into itself.
/// Type compatibility is the application's, through the events it accepts.
fn can_connect(from: OutPin, to: InPin) -> bool {
    from.node != to.node
}

/// Every wire whose drawn polyline the stroke crosses.
fn wires_crossed_by(stroke: &[Pos2], wires: &[WireLayout]) -> Vec<Wire> {
    wires
        .iter()
        .filter(|wire| {
            stroke
                .windows(2)
                .any(|segment| polyline_crosses_segment(&wire.polyline, segment[0], segment[1]))
        })
        .map(|wire| wire.wire)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{InputId, OutputId};
    use egui::{Color32, pos2};

    fn wire_layout(index: u64, polyline: Vec<Pos2>) -> WireLayout {
        WireLayout {
            wire: Wire {
                from: OutPin {
                    node: NodeId(index),
                    output: OutputId(0),
                },
                to: InPin {
                    node: NodeId(index + 100),
                    input: InputId(0),
                },
            },
            polyline,
            color: Color32::WHITE,
        }
    }

    #[test]
    fn a_stroke_cuts_every_wire_it_crosses_and_no_other() {
        let wires = [
            wire_layout(0, vec![pos2(0.0, 0.0), pos2(100.0, 0.0)]),
            wire_layout(1, vec![pos2(0.0, 50.0), pos2(100.0, 50.0)]),
            wire_layout(2, vec![pos2(0.0, 500.0), pos2(100.0, 500.0)]),
        ];
        // Down across the first two, bending away before the third.
        let stroke = [pos2(50.0, -10.0), pos2(50.0, 60.0), pos2(300.0, 60.0)];

        let crossed = wires_crossed_by(&stroke, &wires);

        assert_eq!(crossed, vec![wires[0].wire, wires[1].wire]);
    }

    #[test]
    fn a_single_point_is_no_stroke() {
        let wires = [wire_layout(0, vec![pos2(0.0, 0.0), pos2(100.0, 0.0)])];
        assert!(wires_crossed_by(&[pos2(50.0, 0.0)], &wires).is_empty());
    }
}
