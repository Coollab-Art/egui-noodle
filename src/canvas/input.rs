//! Turning this frame's egui responses into gestures: where the hit areas are
//! registered, and what each press, drag and key does to the gesture in
//! progress. Every effect on the graph leaves as a `CanvasEvent`.

use super::gestures::{Gesture, Interaction, Interactors};
use super::{
    Canvas, CanvasEvent, CanvasStyle, Selection, WireLayout, polyline_crosses_segment,
    segment_intersects_rect, snap,
};
use crate::{AnyPin, Graph, NodeId, Wire};
use egui::{Key, PointerButton, Pos2, Rect, Response, Sense, Ui, emath::TSTransform};

impl Canvas {
    /// Registers this frame's hit areas from where everything was drawn last
    /// frame - in phase with egui, which hit-tests against last frame's rects
    /// too. They go on the nodes layer, scaled like it, so a widget inside a
    /// node is registered later than its frame and wins the tie, and a
    /// slider stays a slider; pins come after the frames, so they win the
    /// node's edge. Under the command modifier a frame only senses clicks:
    /// click toggles the selection, while a drag falls through to the
    /// background. See `4-Input Arbitration.md`.
    pub(super) fn register_interactors<N>(
        &self,
        nodes_ui: &mut Ui,
        graph: &Graph<N>,
        modifiers: egui::Modifiers,
        content_scale: f32,
        style: &CanvasStyle,
    ) -> (Interactors<NodeId>, Interactors<AnyPin>) {
        let frame_sense = if modifiers.command {
            Sense::click()
        } else {
            Sense::click_and_drag()
        };
        let to_scaled = TSTransform::from_scaling(content_scale);
        // Off-screen nodes cannot be pointed at - except the ones the gesture
        // in progress hangs off, which keep their interactors so that panning
        // them off-screen mid-drag does not read as the drag ending.
        let dragged = self.gesture.nodes();
        let visible = self
            .draw_order
            .iter()
            .filter(|id| graph.contains(**id))
            .filter_map(|id| Some((*id, self.layout.nodes.get(id)?)))
            .filter(|(id, node)| !node.culled || dragged.contains(id));
        let mut frames = Vec::new();
        let mut pins = Vec::new();
        for (id, node) in visible {
            frames.push((
                id,
                nodes_ui.interact(
                    to_scaled * node.rect,
                    nodes_ui.id().with(("frame", id)),
                    frame_sense,
                ),
            ));
            for (pin, rect) in node.pins(id) {
                pins.push((
                    pin,
                    nodes_ui.interact(
                        to_scaled * rect.expand(style.pin_hit_expansion),
                        nodes_ui.id().with(("pin", pin)),
                        Sense::click_and_drag(),
                    ),
                ));
            }
        }
        (frames, pins)
    }

    /// Reads this frame's responses, updates the gesture and the selection,
    /// and reports what changed. The order of the four phases is the order the
    /// user perceives: a press on a widget wins over the same press reaching
    /// the background behind it.
    pub(super) fn handle_input<N>(
        &mut self,
        graph: &Graph<N>,
        interaction: Interaction<'_>,
        style: &CanvasStyle,
        events: &mut Vec<CanvasEvent>,
    ) {
        self.selection.retain_present(graph);
        self.press_on_widget(graph, interaction, events);
        self.advance_gesture(graph, interaction, style, events);
        self.press_on_background(graph, interaction, events);
        self.delete_key(graph, interaction.background, events);
    }

    /// A press on a node's frame or on a pin: what it selects, what it raises,
    /// and the gesture it begins. The `+` on a wire is here too - it is a
    /// click on a widget, even though it begins nothing.
    fn press_on_widget<N>(
        &mut self,
        graph: &Graph<N>,
        interaction: Interaction<'_>,
        events: &mut Vec<CanvasEvent>,
    ) {
        let Interaction {
            frames,
            pins,
            insert_button,
            pointer,
            modifiers,
            ..
        } = interaction;

        for (id, response) in frames {
            if response.clicked() {
                if modifiers.command {
                    self.selection.toggle_node(*id);
                } else {
                    self.select_only(*id);
                }
                self.raise(*id);
            }
            if response.drag_started_by(PointerButton::Primary) {
                if !self.selection.contains_node(*id) {
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
                self.gesture = Gesture::DraggingWire {
                    origin: *pin,
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
    }

    /// Moves the gesture in progress along with the pointer, and settles it
    /// when it ends.
    fn advance_gesture<N>(
        &mut self,
        graph: &Graph<N>,
        interaction: Interaction<'_>,
        style: &CanvasStyle,
        events: &mut Vec<CanvasEvent>,
    ) {
        let Interaction {
            frames,
            pins,
            background,
            pointer,
            modifiers,
            ..
        } = interaction;

        // Taken out for the duration, so the arms can borrow the rest of `self`.
        let mut gesture = std::mem::take(&mut self.gesture);
        let ended = match &mut gesture {
            Gesture::Idle => false,
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
                        snap::Snap::default()
                    } else {
                        self.snap_targets(graph, targets, style)
                    };
                    for target in targets.values_mut() {
                        *target += snapped.delta;
                    }
                    *guide_x = snapped.guide_x;
                    *guide_y = snapped.guide_y;
                    *insert_target = self.insert_target_under(pointer, targets, modifiers, style);
                }
                let ended = drag_ended(frames, handle);
                if ended {
                    self.release_nodes(start, targets, *insert_target, events);
                }
                ended
            }
            Gesture::BoxSelecting {
                start,
                current,
                preview,
            } => {
                if let Some(pointer) = pointer {
                    *current = pointer;
                }
                *preview = self
                    .layout
                    .contents_of(Rect::from_two_pos(*start, *current));
                if modifiers.shift {
                    preview.extend(&self.selection);
                }
                let ended = background.drag_stopped();
                if ended {
                    self.selection = std::mem::take(preview);
                }
                ended
            }
            Gesture::DraggingWire { origin, current } => {
                if let Some(pointer) = pointer {
                    *current = pointer;
                }
                let ended = drag_ended(pins, origin);
                if ended {
                    let target = self.layout.pin_at(*current, style.pin_hit_expansion);
                    self.release_wire(*origin, target, *current, events);
                }
                ended
            }
            Gesture::Cutting {
                stroke,
                wires,
                nodes,
            } => {
                if let Some(pointer) = pointer {
                    self.extend_cut(stroke, wires, nodes, pointer);
                }
                let ended = background.drag_stopped();
                if ended && (!wires.is_empty() || !nodes.is_empty()) {
                    events.push(self.delete_request(
                        graph,
                        std::mem::take(nodes),
                        std::mem::take(wires),
                    ));
                }
                ended
            }
        };
        self.gesture = if ended { Gesture::Idle } else { gesture };
    }

    /// A press or click that reached the canvas behind every widget: it starts
    /// a box selection or a cut stroke, picks a wire out or clears the
    /// selection, splices into a wire, or asks for the application's menu.
    fn press_on_background<N>(
        &mut self,
        graph: &Graph<N>,
        interaction: Interaction<'_>,
        events: &mut Vec<CanvasEvent>,
    ) {
        let Interaction {
            background,
            pointer,
            modifiers,
            ..
        } = interaction;

        if background.drag_started_by(PointerButton::Primary)
            && let Some(pointer) = pointer
        {
            // Under the command modifier node frames only sense clicks, so this
            // drag reaches the background even when it began over a node.
            self.gesture = if modifiers.command {
                Gesture::Cutting {
                    stroke: vec![pointer],
                    wires: Vec::new(),
                    nodes: Vec::new(),
                }
            } else {
                Gesture::BoxSelecting {
                    start: pointer,
                    current: pointer,
                    preview: Selection::default(),
                }
            };
        }
        if background.clicked() {
            match self.hovered.wire {
                Some(wire) if modifiers.command => self.selection.toggle_wire(wire),
                Some(wire) => {
                    self.selection.clear();
                    self.selection.wires.insert(wire);
                }
                None if !modifiers.command => self.selection.clear(),
                None => {}
            }
        }
        if background.double_clicked()
            && let Some(wire) = self.hovered.wire
            && let Some(pos) = pointer
        {
            events.push(CanvasEvent::WireInsertRequested { wire, pos });
        }
        if background.secondary_clicked()
            && let Some(pos) = pointer
        {
            match self.hovered.wire {
                Some(wire) => events.push(self.delete_request(graph, Vec::new(), vec![wire])),
                None => events.push(CanvasEvent::ContextMenuRequested { pos }),
            }
        }
    }

    /// Delete and Backspace, which only apply while the pointer is over the
    /// canvas and nothing else wants the keyboard.
    fn delete_key<N>(
        &self,
        graph: &Graph<N>,
        background: &Response,
        events: &mut Vec<CanvasEvent>,
    ) {
        let delete_pressed = background
            .ctx
            .input(|input| input.key_pressed(Key::Delete) || input.key_pressed(Key::Backspace));
        if delete_pressed
            && background.contains_pointer()
            && !background.ctx.egui_wants_keyboard_input()
            && !self.selection.is_empty()
        {
            events.push(self.delete_request(
                graph,
                self.selection.nodes.iter().copied().collect(),
                self.selection.wires.iter().copied().collect(),
            ));
        }
    }

    /// Grows the cut stroke to the pointer and marks everything the new
    /// segment crossed. Nothing already marked is re-tested, and nothing
    /// off-screen can be cut, since it was not drawn to cross.
    fn extend_cut(
        &self,
        stroke: &mut Vec<Pos2>,
        wires: &mut Vec<Wire>,
        nodes: &mut Vec<NodeId>,
        pointer: Pos2,
    ) {
        if stroke.last() == Some(&pointer) {
            return;
        }
        stroke.push(pointer);
        // Only the newest segment can cross anything new.
        let (a, b) = (stroke[stroke.len() - 2], stroke[stroke.len() - 1]);
        for wire in wires_crossed_by(&[a, b], &self.layout.wires) {
            if !wires.contains(&wire) {
                wires.push(wire);
            }
        }
        for (id, node) in &self.layout.nodes {
            if !node.culled && !nodes.contains(id) && segment_intersects_rect(a, b, node.rect) {
                nodes.push(*id);
            }
        }
    }
}

/// Whether the drag that started on `key`'s interactor has ended - including
/// by that interactor no longer existing.
fn drag_ended<K: PartialEq>(responses: &[(K, Response)], key: &K) -> bool {
    responses
        .iter()
        .find(|(other, _)| other == key)
        .is_none_or(|(_, response)| response.drag_stopped())
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
    use crate::{InPin, InputId, OutPin, OutputId};
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
            from: polyline[0],
            to: polyline[polyline.len() - 1],
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
