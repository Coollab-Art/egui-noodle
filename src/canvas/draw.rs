//! Building and painting one frame: every node drawn back to front, then the
//! wires routed between the pins that came out of it. What this produces is
//! the `GraphLayout` the next frame reads.

use super::{
    Canvas, CanvasStyle, NodeContent, NodeLayout, WireEndpoints, WireLayout, node_ui, overlay,
    round_corners, route_wire,
};
use crate::{Graph, NodeId};
use egui::{Rect, Shape, Stroke, Ui};
use std::collections::BTreeMap;

/// How far past the panel a node may sit before its content is skipped.
/// Generous, so a node half a screen away still animates in smoothly.
const CULL_MARGIN: f32 = 200.0;

impl Canvas {
    /// Builds every node's content - or, for a node off-screen, places its
    /// last geometry at its position without building anything.
    pub(super) fn draw_nodes<C: NodeContent>(
        &self,
        nodes_ui: &mut Ui,
        graph: &mut Graph<C::Node>,
        content: &mut C,
        content_scale: f32,
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
                nodes_ui,
                style,
                content_scale,
                content,
                &mut node.payload,
                node_ui::NodePlacement {
                    id: *id,
                    pos,
                    known_size: previous.map(|previous| previous.rect.size()),
                },
            );
            // With the node, not on the overlay: they are its own, so a node
            // drawn in front of it must cover them.
            node_ui::draw_marks(
                nodes_ui.painter(),
                style,
                content_scale,
                *id,
                &drawn,
                &node_ui::NodeMarks {
                    outline: overlay::node_outline(
                        *id,
                        &drawn,
                        self.shown_selection(),
                        &self.gesture,
                        style,
                    ),
                    hovered_pin: self.hovered.pin,
                },
            );
            if previous.is_none() {
                // Laid out against a placeholder size: do the frame again with
                // the real one before anyone sees it.
                nodes_ui
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
    pub(super) fn route_wires<N>(
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
                    // The source pin's colour: a wire carries what the output
                    // produces, whatever the input it feeds is drawn as.
                    color: from_pin.color,
                })
            })
            .collect()
    }

    /// Every wire in its own colour, with the highlighted ones (selected,
    /// hovered, about to be cut, about to receive a dropped node) outlined
    /// behind it or restroked over it. The outline stands out either side by
    /// as much as a selected node's halo, so the two read as the same mark.
    pub(super) fn wire_shapes(&self, wires: &[WireLayout], style: &CanvasStyle) -> Shape {
        let highlights: Vec<Option<overlay::WireHighlight>> = wires
            .iter()
            .map(|wire| {
                overlay::wire_highlight(
                    wire,
                    self.shown_selection(),
                    &self.gesture,
                    self.hovered,
                    style,
                )
            })
            .collect();
        let mut shapes = Vec::with_capacity(wires.len());
        for (wire, highlight) in wires.iter().zip(&highlights) {
            if let Some(overlay::WireHighlight::Outline(color)) = highlight {
                shapes.push(Shape::line(
                    wire.polyline.clone(),
                    Stroke::new(style.wire_width + style.selection_width, *color),
                ));
            }
        }
        for wire in wires {
            shapes.push(Shape::line(
                wire.polyline.clone(),
                Stroke::new(style.wire_width, wire.color),
            ));
        }
        for (wire, highlight) in wires.iter().zip(&highlights) {
            if let Some(overlay::WireHighlight::Restroke(color)) = highlight {
                shapes.push(Shape::line(
                    wire.polyline.clone(),
                    Stroke::new(style.wire_width * 1.5, *color),
                ));
            }
        }
        Shape::Vec(shapes)
    }
}

/// Multi-frame layout: what one node measures, fed back as the size it is
/// laid out against next frame. Run through a real `egui::Context`, since the
/// loop only closes through egui's own measuring.
#[cfg(test)]
mod layout_tests {
    use super::*;
    use crate::{ConnectionPolicy, InputId, InputSpec, OutputId, OutputSpec};
    use egui::{Color32, Vec2, pos2};

    struct TestContent;

    impl NodeContent for TestContent {
        type Node = ();

        fn inputs(&mut self, _node: &()) -> Vec<InputSpec> {
            ["center_position_xy", "random_seed", "zoom_amount"]
                .iter()
                .enumerate()
                .map(|(index, label)| InputSpec {
                    id: InputId(index as u64),
                    label: (*label).to_owned(),
                    color: Color32::RED,
                    policy: ConnectionPolicy::Single,
                })
                .collect()
        }

        fn outputs(&mut self, _node: &()) -> Vec<OutputSpec> {
            vec![OutputSpec {
                id: OutputId(0),
                label: "image".to_owned(),
                color: Color32::GREEN,
            }]
        }

        fn title(&mut self, _node: &()) -> String {
            "cloud_eleven_with_a_long_name.fs".to_owned()
        }
    }

    /// The size one node was drawn at, over `frames` consecutive frames.
    fn node_sizes(zoom: f32, pixels_per_point: f32, frames: usize) -> Vec<Vec2> {
        let ctx = egui::Context::default();
        ctx.set_pixels_per_point(pixels_per_point);
        let style = CanvasStyle::default();
        let mut canvas = Canvas::new();
        canvas.view.zoom = zoom;
        let mut graph = Graph::new();
        graph.add_node((), pos2(0.0, 0.0));
        (0..frames)
            .map(|_| {
                ctx.run_ui(Default::default(), |ui| {
                    canvas.show(ui, &mut graph, &mut TestContent, &style);
                })
                .drop_without_applying_deltas();
                canvas
                    .layout()
                    .nodes
                    .values()
                    .next()
                    .map_or(Vec2::ZERO, |node| node.rect.size())
            })
            .collect()
    }

    /// A node is laid out against the size it measured last frame, so any
    /// rounding that does not survive the round trip through `content_scale`
    /// would make it creep every frame. It has to settle instead.
    #[test]
    fn a_nodes_size_settles_and_stays_put() {
        for pixels_per_point in [1.0, 1.25, 1.5, 2.0] {
            for zoom_step in 0..=20 {
                let zoom = 0.2 + zoom_step as f32 * 0.14;
                // Four frames to settle, four more to prove it stays put.
                let sizes = node_sizes(zoom, pixels_per_point, 8);
                let (settled, last) = (sizes[4], sizes[7]);
                assert!(
                    settled == last,
                    "at {pixels_per_point} pixels per point and zoom {zoom}, a node grew from {settled:?} to {last:?}"
                );
            }
        }
    }
}
