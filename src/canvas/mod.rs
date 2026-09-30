//! The canvas: draws a `Graph` and reports what the user did to it. It owns
//! only view state - pan, zoom, selection, the gesture in progress, and what
//! it drew last frame - never the graph.

mod content;
mod draw;
mod events;
mod gestures;
mod grid;
mod input;
mod layout;
mod node_ui;
mod overlay;
mod room;
mod selection;
mod snap;
mod style;
mod view;
mod wires;

pub use content::*;
pub use events::*;
pub use gestures::Hovered;
pub use layout::*;
pub use node_ui::content_scale;
pub use selection::*;
pub use style::*;
pub use view::*;
pub use wires::*;

use crate::{Graph, NodeIdentifier};
use egui::{LayerId, Painter, Pos2, Rect, Sense, Shape, Ui, UiBuilder, Vec2, emath::TSTransform};
use gestures::{Gesture, Interaction};
use room::Room;
use std::collections::{BTreeMap, BTreeSet};

/// One node-graph view. Keep it across frames; hand it the graph each frame.
pub struct Canvas<I> {
    pub view: ViewState,
    /// What was drawn last frame. Gestures and application hit-testing read
    /// this, one frame behind; it is also where an off-screen node's geometry
    /// comes from when its content is not built.
    layout: GraphLayout<I>,
    /// Back to front. Nodes the graph gained are appended on top; nodes it
    /// lost are dropped.
    draw_order: Vec<I>,
    /// The graph's topology revision `draw_order` was last synced against.
    synced_topology: Option<u64>,
    selection: Selection<I>,
    gesture: Gesture<I>,
    hovered: Hovered<I>,
    room: Option<Room<I>>,
    /// Where the nodes reported in this frame's `NodesMoved` are drawn, since
    /// the graph only learns of the move after `show` returns. Without it a
    /// dropped node would flash back to where it started for one frame. See
    /// `1-Application Owns the Model.md` on the transient offset.
    settled: BTreeMap<I, Pos2>,
}

pub struct CanvasResponse<I> {
    /// In the order they happened. Apply them to the graph, or turn them
    /// into commands.
    pub events: Vec<CanvasEvent<I>>,
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

// By hand: a derive would demand `I: Default`, which an id has no reason to be.
impl<I> Default for Canvas<I> {
    fn default() -> Self {
        Self {
            view: ViewState::default(),
            layout: GraphLayout::default(),
            draw_order: Vec::new(),
            synced_topology: None,
            selection: Selection::default(),
            gesture: Gesture::default(),
            hovered: Hovered::default(),
            room: None,
            settled: BTreeMap::new(),
        }
    }
}

impl<I: NodeIdentifier> Canvas<I> {
    pub fn new() -> Self {
        Self::default()
    }

    /// Where everything was drawn last frame.
    pub fn layout(&self) -> &GraphLayout<I> {
        &self.layout
    }

    pub fn selection(&self) -> &Selection<I> {
        &self.selection
    }

    pub fn select_only(&mut self, id: I) {
        self.selection.clear();
        self.selection.nodes.insert(id);
    }

    /// Selects these nodes and nothing else.
    pub fn set_selection(&mut self, nodes: impl IntoIterator<Item = I>) {
        self.selection = Selection {
            nodes: nodes.into_iter().collect(),
            wires: BTreeSet::new(),
        };
    }

    /// What the pointer is over while nothing is being dragged.
    pub fn hovered(&self) -> Hovered<I> {
        self.hovered
    }

    /// The size a node would be drawn at, without drawing it: laid out on an
    /// invisible child of `ui`, at zoom 1. For placing a node before it
    /// exists - centring it on a point, or making room for it in a wire
    /// ([`Canvas::make_room`]) - rather than guessing from `default_node_size`.
    ///
    /// Laid out the way a shown node is: a sizing pass first, then against
    /// the size that came out, until it stops changing. One pass alone can be
    /// a few pixels off what the node settles at once it is drawn.
    pub fn measure_node<C: NodeContent<Id = I>>(
        &self,
        ui: &mut Ui,
        content: &mut C,
        id: I,
        node: &C::Node,
        style: &CanvasStyle,
    ) -> Vec2 {
        const SETTLE_PASSES: usize = 4;
        let mut known_size = None;
        for _ in 0..SETTLE_PASSES {
            let mut measuring_ui = ui.new_child(
                UiBuilder::new()
                    .id_salt("egui_noodle_measure")
                    .max_rect(Rect::EVERYTHING)
                    .invisible(),
            );
            let size = node_ui::draw_node(
                &mut measuring_ui,
                style,
                1.0,
                content,
                node,
                node_ui::NodePlacement {
                    id,
                    pos: Pos2::ZERO,
                    known_size,
                },
            )
            .rect
            .size();
            if known_size == Some(size) {
                break;
            }
            known_size = Some(size);
        }
        known_size.unwrap_or(style.default_node_size)
    }

    pub fn show<C: NodeContent<Id = I>>(
        &mut self,
        ui: &mut Ui,
        graph: &Graph<C::Node, I>,
        content: &mut C,
        style: &CanvasStyle,
    ) -> CanvasResponse<I> {
        let panel_rect = ui.available_rect_before_wrap();
        ui.painter().rect_filled(panel_rect, 0.0, style.background);
        // Last frame's drops have reached the graph by now.
        self.settled.clear();

        // Three paint layers, transformed by egui so that everything drawn and
        // every rect interacted with below is in graph space: the base (grid,
        // wires, background response), the nodes, and the overlay. The nodes
        // get their own because their content is laid out at the zoom when
        // zoomed in, so text is rasterised at the size it is shown at instead
        // of magnified - see `content_scale` and `5-Crisp Text.md`.
        // Direct sublayers of the panel's layer, not chained: egui keeps
        // siblings in the order first seen, while nested sublayers are
        // unspecified.
        let parent_layer = ui.layer_id();
        let base_layer = LayerId::new(parent_layer.order, ui.id().with("egui_noodle_canvas"));
        let nodes_layer = LayerId::new(parent_layer.order, ui.id().with("egui_noodle_nodes"));
        let overlay_layer = LayerId::new(parent_layer.order, ui.id().with("egui_noodle_overlay"));
        for layer in [base_layer, nodes_layer, overlay_layer] {
            ui.ctx().set_sublayer(parent_layer, layer);
        }
        let mut canvas_ui = ui.new_child(
            UiBuilder::new()
                .layer_id(base_layer)
                .max_rect(Rect::EVERYTHING)
                .sense(Sense::click_and_drag()),
        );
        let background = canvas_ui.response();
        // Whether the pointer is on the panel with nothing egui owns (a popup,
        // a window) over it. Not the background's `contains_pointer`: nodes and
        // pins sit on their own sublayer, which would hide the pointer from it.
        let over_canvas = ui.ctx().rect_contains_pointer(parent_layer, panel_rect);
        self.navigate(&canvas_ui, &background, panel_rect, over_canvas, style);

        let to_global = self.view.to_global(panel_rect);
        let content_scale = self.view.zoom.max(1.0);
        let nodes_to_global = to_global * TSTransform::from_scaling(1.0 / content_scale);
        let viewport = to_global.inverse() * panel_rect;
        let clip_rect = panel_rect.intersect(ui.clip_rect());
        canvas_ui.set_clip_rect(to_global.inverse() * clip_rect);
        canvas_ui.ctx().set_transform_layer(base_layer, to_global);
        let mut nodes_ui = ui.new_child(
            UiBuilder::new()
                .layer_id(nodes_layer)
                .max_rect(Rect::EVERYTHING),
        );
        nodes_ui.set_clip_rect(nodes_to_global.inverse() * clip_rect);
        nodes_ui
            .ctx()
            .set_transform_layer(nodes_layer, nodes_to_global);
        nodes_ui.ctx().data_mut(|data| {
            data.insert_temp(node_ui::content_scale_key(nodes_layer), content_scale);
        });
        let overlay_painter = Painter::new(
            ui.ctx().clone(),
            overlay_layer,
            to_global.inverse() * clip_rect,
        );
        ui.ctx().set_transform_layer(overlay_layer, to_global);

        self.sync_draw_order(graph);

        let modifiers = canvas_ui.input(|input| input.modifiers);
        let (frames, pins) =
            self.register_interactors(&mut nodes_ui, graph, modifiers, content_scale, style);
        let (pointer, press_origin) = canvas_ui.input(|input| {
            (
                input
                    .pointer
                    .latest_pos()
                    .map(|pos| to_global.inverse() * pos),
                input
                    .pointer
                    .press_origin()
                    .map(|pos| to_global.inverse() * pos),
            )
        });
        self.update_hovered(pointer, over_canvas, style);
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
            Interaction {
                frames: &frames,
                pins: &pins,
                insert_button: insert_button.as_ref(),
                background: &background,
                pointer,
                press_origin,
                modifiers,
            },
            style,
            &mut events,
        );
        self.advance_slide(canvas_ui.ctx());

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

        let nodes = self.draw_nodes(
            &mut nodes_ui,
            graph,
            content,
            content_scale,
            style,
            viewport,
        );
        let wires = self.route_wires(graph, &nodes, to_global.scaling, style);
        canvas_ui
            .painter()
            .set(wires_slot, self.wire_shapes(&wires, style));

        let layout = GraphLayout {
            to_global,
            viewport,
            nodes,
            draw_order: self.draw_order.clone(),
            wires,
        };
        overlay::draw_overlay(
            &overlay_painter,
            overlay::OverlayInput {
                layout: &layout,
                gesture: &self.gesture,
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
        CanvasResponse { events, stats }
    }

    fn sync_draw_order<N>(&mut self, graph: &Graph<N, I>) {
        if self.synced_topology == Some(graph.topology_revision()) {
            return;
        }
        self.synced_topology = Some(graph.topology_revision());
        self.draw_order.retain(|id| graph.contains(*id));
        let present: BTreeSet<I> = self.draw_order.iter().copied().collect();
        for (id, _) in graph.nodes() {
            if !present.contains(&id) {
                self.draw_order.push(id);
            }
        }
    }
}
