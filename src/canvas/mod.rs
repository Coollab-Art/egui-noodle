//! The canvas: draws a `Graph` and reports what the user did to it. It owns
//! only view state - pan, zoom, selection, the gesture in progress, and what
//! it drew last frame - never the graph.

mod content;
mod events;
mod gestures;
mod grid;
mod layout;
mod node_ui;
mod overlay;
mod room;
mod snap;
mod style;
mod view;
mod wires;

pub use content::*;
pub use events::*;
pub use gestures::Hovered;
pub use layout::*;
pub use style::*;
pub use view::*;
pub use wires::*;

use crate::{AnyPin, Graph, NodeId};
use egui::{LayerId, PointerButton, Rect, Response, Sense, Shape, Stroke, Ui, UiBuilder, Vec2};
use gestures::{Gesture, Interactors};
use room::Room;
use std::collections::{BTreeMap, BTreeSet};

/// One node-graph view. Keep it across frames; hand it the graph each frame.
#[derive(Default)]
pub struct Canvas {
    pub view: ViewState,
    /// What was drawn last frame. Gestures and application hit-testing read
    /// this, one frame behind; it is also where an off-screen node's geometry
    /// comes from when its content is not built.
    layout: GraphLayout,
    /// Back to front. Nodes the graph gained are appended on top; nodes it
    /// lost are dropped.
    draw_order: Vec<NodeId>,
    /// The graph's topology revision `draw_order` was last synced against.
    synced_topology: Option<u64>,
    selection: BTreeSet<NodeId>,
    gesture: Gesture,
    hovered: Hovered,
    room: Option<Room>,
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

    /// What the pointer is over while nothing is being dragged.
    pub fn hovered(&self) -> Hovered {
        self.hovered
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

        let modifiers = canvas_ui.input(|input| input.modifiers);
        let (frames, pins) = self.register_interactors(&mut canvas_ui, graph, modifiers, style);
        let pointer = canvas_ui
            .input(|input| input.pointer.latest_pos())
            .map(|pointer| to_global.inverse() * pointer);
        self.update_hovered(pointer, background.contains_pointer(), style);
        let insert_button = self
            .hovered
            .wire
            .and_then(|wire| {
                self.layout
                    .wire(wire)?
                    .insert_button_rect(style.insert_button_size)
            })
            .map(|rect| {
                let response =
                    canvas_ui.interact(rect, canvas_ui.id().with("insert_button"), Sense::click());
                (rect, response)
            });

        let mut events = Vec::new();
        self.handle_input(
            graph,
            gestures::Interaction {
                frames: &frames,
                pins: &pins,
                insert_button: insert_button.as_ref(),
                background: &background,
                pointer,
                modifiers,
            },
            style,
            &mut events,
        );
        self.advance_slide(canvas_ui.ctx(), graph, &mut events);

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

        let nodes = self.draw_nodes(&mut canvas_ui, graph, content, style, viewport);
        let wires = self.route_wires(graph, &nodes, to_global.scaling, style);
        canvas_ui.painter().set(
            wires_slot,
            Shape::Vec(
                wires
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

        let layout = GraphLayout {
            to_global,
            viewport,
            nodes,
            draw_order: self.draw_order.clone(),
            wires,
        };
        overlay::draw_overlay(
            canvas_ui.painter(),
            overlay::OverlayInput {
                layout: &layout,
                selection: &self.selection,
                gesture: &self.gesture,
                hovered: self.hovered,
                insert_button: insert_button
                    .as_ref()
                    .map(|(rect, response)| (*rect, response.hovered())),
                zoom: self.view.zoom,
            },
            style,
        );

        // So a click or drag anywhere on the panel reaches the background.
        canvas_ui.expand_to_include_rect(viewport);

        let stats = CanvasStats {
            nodes: layout.nodes.len(),
            culled: layout.nodes.values().filter(|node| node.culled).count(),
            wires: layout.wires.len(),
        };
        self.layout = layout;
        self.resolve_waiting_room(graph, style);
        CanvasResponse { events, stats }
    }

    /// Pan and zoom from the background's input. Middle or secondary drag
    /// pans and leaves the primary button free for selection; an unmodified
    /// wheel zooms around the pointer; a modified wheel (shift or alt, which
    /// egui turns into horizontal or vertical scroll) pans; ctrl and pinch
    /// zoom through egui's own `zoom_delta`.
    fn navigate(&mut self, ui: &Ui, background: &Response, panel_rect: Rect, style: &CanvasStyle) {
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
            // The curve egui's `InputState` applies to a modified wheel, so an
            // unmodified one zooms at the same rate.
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
        if self.synced_topology == Some(graph.topology_revision()) {
            return;
        }
        self.synced_topology = Some(graph.topology_revision());
        self.draw_order.retain(|id| graph.contains(*id));
        let present: BTreeSet<NodeId> = self.draw_order.iter().copied().collect();
        for (id, _) in graph.nodes() {
            if !present.contains(&id) {
                self.draw_order.push(id);
            }
        }
    }

    /// Registers this frame's hit areas from where everything was drawn last
    /// frame - in phase with egui, which hit-tests against last frame's rects
    /// too. Node frames are registered before any content, so a widget inside
    /// a node wins the tie and a slider stays a slider; pins come after the
    /// frames, so they win the node's edge. Under the command modifier a
    /// frame only senses clicks: click toggles the selection, while a drag
    /// falls through to the background. See `post-mortems/4-Input Arbitration.md`.
    fn register_interactors<N>(
        &self,
        canvas_ui: &mut Ui,
        graph: &Graph<N>,
        modifiers: egui::Modifiers,
        style: &CanvasStyle,
    ) -> (Interactors<NodeId>, Interactors<AnyPin>) {
        let frame_sense = if modifiers.command {
            Sense::click()
        } else {
            Sense::click_and_drag()
        };
        // Off-screen nodes cannot be pointed at.
        let visible = self
            .draw_order
            .iter()
            .filter(|id| graph.contains(**id))
            .filter_map(|id| Some((*id, self.layout.nodes.get(id)?)))
            .filter(|(_, node)| !node.culled);
        let mut frames = Vec::new();
        let mut pins = Vec::new();
        for (id, node) in visible {
            frames.push((
                id,
                canvas_ui.interact(node.rect, canvas_ui.id().with(("frame", id)), frame_sense),
            ));
            for (pin, rect) in node.pins(id) {
                pins.push((
                    pin,
                    canvas_ui.interact(
                        rect.expand(style.pin_hit_expansion),
                        canvas_ui.id().with(("pin", pin)),
                        Sense::click_and_drag(),
                    ),
                ));
            }
        }
        (frames, pins)
    }

    /// Builds every node's content - or, for a node off-screen, places its
    /// last geometry at its position without building anything.
    fn draw_nodes<C: NodeContent>(
        &mut self,
        canvas_ui: &mut Ui,
        graph: &mut Graph<C::Node>,
        content: &mut C,
        style: &CanvasStyle,
        viewport: Rect,
    ) -> BTreeMap<NodeId, NodeLayout> {
        let cull_bounds = viewport.expand(CULL_MARGIN);
        let mut nodes = BTreeMap::new();
        for id in &self.draw_order {
            let Some(node) = graph.node_mut(*id) else {
                continue;
            };
            let pos = self.drawn_pos(*id, node.pos);
            let previous = self.layout.nodes.get(id);
            if let Some(previous) = previous
                && !Rect::from_min_size(pos, previous.rect.size()).intersects(cull_bounds)
            {
                nodes.insert(*id, previous.translated(pos - previous.rect.min, true));
                continue;
            }
            let drawn = node_ui::draw_node(
                canvas_ui,
                style,
                content,
                *id,
                &mut node.payload,
                pos,
                previous.map(|previous| previous.rect.size()),
            );
            if previous.is_none() {
                // Laid out against a placeholder size: do the frame again with
                // the real one before anyone sees it.
                canvas_ui
                    .ctx()
                    .request_discard("egui-noodle: first layout of a node");
            }
            nodes.insert(*id, drawn);
        }
        nodes
    }

    /// Routes every wire between this frame's pins, reusing last frame's
    /// polyline when neither end nor either node moved and the zoom is the
    /// same - which is nearly every wire, nearly every frame.
    fn route_wires<N>(
        &mut self,
        graph: &Graph<N>,
        nodes: &BTreeMap<NodeId, NodeLayout>,
        zoom: f32,
        style: &CanvasStyle,
    ) -> Vec<WireLayout> {
        let same_zoom = self.layout.to_global.scaling == zoom;
        let mut previous: BTreeMap<crate::Wire, WireLayout> =
            std::mem::take(&mut self.layout.wires)
                .into_iter()
                .map(|wire| (wire.wire, wire))
                .collect();
        let tolerance = style.wire_tolerance / zoom;
        graph
            .wires()
            .iter()
            .filter_map(|wire| {
                let from_node = nodes.get(&wire.from.node)?;
                let to_node = nodes.get(&wire.to.node)?;
                let from_pin = from_node
                    .outputs
                    .iter()
                    .find(|pin| pin.id == wire.from.output)?;
                let to_pin = to_node.inputs.iter().find(|pin| pin.id == wire.to.input)?;
                let (from, to) = (from_pin.rect.center(), to_pin.rect.center());

                let node_unmoved = |id: NodeId| {
                    self.layout.nodes.get(&id).map(|node| node.rect)
                        == nodes.get(&id).map(|node| node.rect)
                };
                let reusable = previous.remove(wire).filter(|last| {
                    same_zoom
                        && last.from == from
                        && last.to == to
                        && node_unmoved(wire.from.node)
                        && node_unmoved(wire.to.node)
                });
                let polyline = match reusable {
                    Some(last) => last.polyline,
                    None => round_corners(
                        &route_wire(
                            &WireEndpoints {
                                from,
                                to,
                                from_node: from_node.rect,
                                to_node: to_node.rect,
                            },
                            &style.wire_routing,
                            0.0,
                        ),
                        style.wire_routing.corner_radius,
                        tolerance,
                    ),
                };
                Some(WireLayout {
                    wire: *wire,
                    from,
                    to,
                    polyline,
                    color: from_pin.color.lerp_to_gamma(to_pin.color, 0.5),
                })
            })
            .collect()
    }
}
