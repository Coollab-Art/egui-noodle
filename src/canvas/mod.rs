//! The canvas: draws a `Graph` and reports what the user did to it. It owns
//! only view state - pan, zoom, selection, the gesture in progress, and what
//! it measured last frame - never the graph.

mod content;
mod events;
mod gestures;
mod grid;
mod layout;
mod node_ui;
mod overlay;
mod snap;
mod style;
mod view;
mod wires;

pub use content::*;
pub use events::*;
pub use layout::*;
pub use style::*;
pub use view::*;
pub use wires::*;

use crate::{Graph, InputId, NodeId, OutputId};
use egui::{LayerId, PointerButton, Rect, Sense, Shape, Stroke, Ui, UiBuilder, Vec2};
use gestures::Gesture;
use std::collections::{BTreeMap, BTreeSet};

/// One node-graph view. Keep it across frames; hand it the graph each frame.
#[derive(Default)]
pub struct Canvas {
    pub view: ViewState,
    /// What was drawn last frame. Gestures and application hit-testing read
    /// this, one frame behind.
    layout: GraphLayout,
    /// Back to front. Nodes the graph gained are appended on top; nodes it
    /// lost are dropped.
    draw_order: Vec<NodeId>,
    /// Each node's size and pin placement the last time it was built, so an
    /// off-screen node can be laid out without building its content.
    measures: BTreeMap<NodeId, NodeMeasure>,
    selection: BTreeSet<NodeId>,
    gesture: Gesture,
}

struct NodeMeasure {
    size: Vec2,
    /// Relative to the node's top-left.
    inputs: Vec<PinLayout<InputId>>,
    outputs: Vec<PinLayout<OutputId>>,
}

pub struct CanvasResponse {
    /// In the order they happened. Apply them to the graph, or turn them
    /// into commands.
    pub events: Vec<CanvasEvent>,
    pub stats: CanvasStats,
}

/// Enough to see what a frame cost. Meant to be shown in a debug overlay.
#[derive(Clone, Copy, Debug, Default)]
pub struct CanvasStats {
    pub nodes: usize,
    /// Nodes whose content was not built because they were off-screen.
    pub culled: usize,
    pub wires: usize,
}

/// How far past the panel a node may sit before its content is skipped.
/// Generous, so a node half a screen away still animates in smoothly.
const CULL_MARGIN: f32 = 200.0;

impl Canvas {
    pub fn new() -> Self {
        Self::default()
    }

    /// Where everything was drawn last frame.
    pub fn layout(&self) -> &GraphLayout {
        &self.layout
    }

    pub fn selection(&self) -> &BTreeSet<NodeId> {
        &self.selection
    }

    pub fn select_only(&mut self, id: NodeId) {
        self.selection.clear();
        self.selection.insert(id);
    }

    pub fn set_selection(&mut self, nodes: impl IntoIterator<Item = NodeId>) {
        self.selection = nodes.into_iter().collect();
    }

    pub fn show<C: NodeContent>(
        &mut self,
        ui: &mut Ui,
        graph: &mut Graph<C::Node>,
        content: &mut C,
        style: &CanvasStyle,
    ) -> CanvasResponse {
        let panel_rect = ui.available_rect_before_wrap();
        ui.painter().rect_filled(panel_rect, 0.0, style.background);

        // Our own paint layer, transformed by egui so that everything drawn
        // and every rect interacted with below is in graph space.
        let layer_id = LayerId::new(ui.layer_id().order, ui.id().with("egui_noodle_canvas"));
        ui.ctx().set_sublayer(ui.layer_id(), layer_id);
        let mut canvas_ui = ui.new_child(
            UiBuilder::new()
                .layer_id(layer_id)
                .max_rect(Rect::EVERYTHING)
                .sense(Sense::click_and_drag()),
        );
        let background = canvas_ui.response();
        self.navigate(&canvas_ui, &background, panel_rect, style);

        let to_global = self.view.to_global(panel_rect);
        let viewport = to_global.inverse() * panel_rect;
        canvas_ui.set_clip_rect(to_global.inverse() * panel_rect.intersect(ui.clip_rect()));
        canvas_ui.ctx().set_transform_layer(layer_id, to_global);

        self.sync_draw_order(graph);

        // Interaction first, from where everything was drawn last frame - in
        // phase with egui, which hit-tests against last frame's rects too.
        // Node frames are registered before any content, so a widget inside a
        // node wins the tie and a slider stays a slider. Under the command
        // modifier a frame only senses clicks: click toggles the selection,
        // while a drag falls through to the background.
        let modifiers = canvas_ui.input(|input| input.modifiers);
        let frame_sense = if modifiers.command {
            Sense::click()
        } else {
            Sense::click_and_drag()
        };
        let frames: Vec<(NodeId, egui::Response)> = self
            .draw_order
            .iter()
            .filter(|id| graph.contains(**id))
            .filter_map(|id| {
                let rect = self.layout.nodes.get(id)?.rect;
                let response =
                    canvas_ui.interact(rect, canvas_ui.id().with(("frame", id)), frame_sense);
                Some((*id, response))
            })
            .collect();
        let pointer = canvas_ui
            .input(|input| input.pointer.latest_pos())
            .map(|pointer| to_global.inverse() * pointer);
        let mut events = Vec::new();
        self.handle_input(
            graph,
            &frames,
            &background,
            pointer,
            modifiers,
            style,
            &mut events,
        );

        grid::draw_grid(
            canvas_ui.painter(),
            viewport,
            style.grid_spacing,
            style.grid_stroke,
            self.view.zoom,
        );
        // Wires go behind nodes but need this frame's pin positions, so their
        // slot is reserved now and filled once the nodes are drawn.
        let wires_slot = canvas_ui.painter().add(Shape::Noop);

        let mut layout = GraphLayout {
            to_global,
            panel_rect,
            viewport,
            nodes: BTreeMap::new(),
            draw_order: self.draw_order.clone(),
            wires: Vec::new(),
        };
        let cull_bounds = viewport.expand(CULL_MARGIN);
        let mut culled = 0;
        for id in &self.draw_order {
            let Some(node) = graph.node_mut(*id) else {
                continue;
            };
            let pos = self.drawn_pos(*id, node.pos);
            let measure = self.measures.get(id);
            if let Some(measure) = measure
                && !Rect::from_min_size(pos, measure.size).intersects(cull_bounds)
            {
                layout.nodes.insert(*id, measure.placed_at(pos));
                culled += 1;
                continue;
            }

            let drawn = node_ui::draw_node(
                &mut canvas_ui,
                style,
                content,
                *id,
                &mut node.payload,
                pos,
                measure.map(|measure| measure.size),
            );
            if measure.is_none() {
                // Laid out against a placeholder size: do the frame again with
                // the real one before anyone sees it.
                canvas_ui
                    .ctx()
                    .request_discard("egui-noodle: first layout of a node");
            }
            self.measures.insert(
                *id,
                NodeMeasure {
                    size: drawn.rect.size(),
                    inputs: translated(&drawn.inputs, -drawn.rect.min.to_vec2()),
                    outputs: translated(&drawn.outputs, -drawn.rect.min.to_vec2()),
                },
            );
            layout.nodes.insert(
                *id,
                NodeLayout {
                    rect: drawn.rect,
                    header_rect: drawn.header_rect,
                    inputs: drawn.inputs,
                    outputs: drawn.outputs,
                    culled: false,
                },
            );
        }
        self.measures.retain(|id, _| graph.contains(*id));

        layout.wires = self.route_wires(graph, &layout, style);
        canvas_ui.painter().set(
            wires_slot,
            Shape::Vec(
                layout
                    .wires
                    .iter()
                    .map(|wire| {
                        Shape::line(
                            wire.polyline.clone(),
                            Stroke::new(style.wire_width, wire.color),
                        )
                    })
                    .collect(),
            ),
        );

        overlay::draw_overlay(
            canvas_ui.painter(),
            &layout,
            &self.selection,
            &self.gesture,
            style,
            self.view.zoom,
        );

        // So a click or drag anywhere on the panel reaches the background.
        canvas_ui.expand_to_include_rect(viewport);

        let stats = CanvasStats {
            nodes: layout.nodes.len(),
            culled,
            wires: layout.wires.len(),
        };
        self.layout = layout;
        CanvasResponse { events, stats }
    }

    /// Pan and zoom from the background's input. Middle or secondary drag
    /// pans and leaves the primary button free for selection; an unmodified
    /// wheel zooms around the pointer; a modified wheel (shift or alt, which
    /// egui turns into horizontal or vertical scroll) pans; ctrl and pinch
    /// zoom through egui's own `zoom_delta`.
    fn navigate(
        &mut self,
        ui: &Ui,
        background: &egui::Response,
        panel_rect: Rect,
        style: &CanvasStyle,
    ) {
        if background.dragged_by(PointerButton::Middle)
            || background.dragged_by(PointerButton::Secondary)
        {
            // The response lives on the transformed layer, so its delta is in
            // graph units.
            self.view
                .pan_by_screen(background.drag_delta() * self.view.zoom);
        }

        let Some(pointer) = ui.input(|input| input.pointer.latest_pos()) else {
            return;
        };
        if !background.contains_pointer() {
            return;
        }
        let (mut zoom_factor, mut scroll, modifiers) = ui.input(|input| {
            (
                input.zoom_delta(),
                input.smooth_scroll_delta(),
                input.modifiers,
            )
        });
        if modifiers.is_none() {
            let speed = ui
                .ctx()
                .options(|options| options.input_options.scroll_zoom_speed);
            zoom_factor *= (speed * (scroll.x + scroll.y)).exp();
            scroll = Vec2::ZERO;
        }
        if zoom_factor != 1.0 {
            self.view
                .zoom_about(pointer, panel_rect, zoom_factor, style.zoom_range);
        }
        if scroll != Vec2::ZERO {
            self.view.pan_by_screen(scroll);
        }
    }

    fn sync_draw_order<N>(&mut self, graph: &Graph<N>) {
        self.draw_order.retain(|id| graph.contains(*id));
        let present: BTreeSet<NodeId> = self.draw_order.iter().copied().collect();
        for (id, _) in graph.nodes() {
            if !present.contains(&id) {
                self.draw_order.push(id);
            }
        }
    }

    fn route_wires<N>(
        &self,
        graph: &Graph<N>,
        layout: &GraphLayout,
        style: &CanvasStyle,
    ) -> Vec<WireLayout> {
        let tolerance = style.wire_tolerance / self.view.zoom;
        graph
            .wires()
            .iter()
            .filter_map(|wire| {
                let from_node = layout.nodes.get(&wire.from.node)?;
                let to_node = layout.nodes.get(&wire.to.node)?;
                let from = from_node
                    .outputs
                    .iter()
                    .find(|pin| pin.id == wire.from.output)?;
                let to = to_node.inputs.iter().find(|pin| pin.id == wire.to.input)?;
                let route = route_wire(
                    &WireEndpoints {
                        from: from.rect.center(),
                        to: to.rect.center(),
                        from_node: from_node.rect,
                        to_node: to_node.rect,
                    },
                    &style.wire_routing,
                    0.0,
                );
                Some(WireLayout {
                    wire: *wire,
                    polyline: round_corners(&route, style.wire_routing.corner_radius, tolerance),
                    color: from.color.lerp_to_gamma(to.color, 0.5),
                })
            })
            .collect()
    }
}

impl NodeMeasure {
    fn placed_at(&self, pos: egui::Pos2) -> NodeLayout {
        NodeLayout {
            rect: Rect::from_min_size(pos, self.size),
            // Not tracked for an unbuilt node; nothing hit-tests a culled header.
            header_rect: Rect::from_min_size(pos, Vec2::new(self.size.x, 0.0)),
            inputs: translated(&self.inputs, pos.to_vec2()),
            outputs: translated(&self.outputs, pos.to_vec2()),
            culled: true,
        }
    }
}

fn translated<Id: Copy>(pins: &[PinLayout<Id>], offset: Vec2) -> Vec<PinLayout<Id>> {
    pins.iter()
        .map(|pin| PinLayout {
            rect: pin.rect.translate(offset),
            ..*pin
        })
        .collect()
}
