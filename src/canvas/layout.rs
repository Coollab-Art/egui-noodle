use super::{Selection, distance_to_polyline, polyline_intersects_rect};
use crate::{AnyPin, ConnectionPolicy, InPin, InputId, NodeId, OutPin, OutputId, Wire};
use egui::{Color32, Pos2, Rect, Vec2, emath::TSTransform};
use std::collections::BTreeMap;

/// Where everything was drawn last frame, in graph space. This is what the
/// application hit-tests against to build its own gestures, and what the
/// canvas's own gestures read - one frame behind, exactly like egui's own
/// hit-testing.
#[derive(Clone, Debug)]
pub struct GraphLayout {
    pub to_global: TSTransform,
    /// Graph space: what the panel showed.
    pub viewport: Rect,
    pub nodes: BTreeMap<NodeId, NodeLayout>,
    /// Back to front.
    pub draw_order: Vec<NodeId>,
    pub wires: Vec<WireLayout>,
}

#[derive(Clone, Debug)]
pub struct NodeLayout {
    pub rect: Rect,
    /// What the header was filled with, which is also what marks the node as
    /// selected under `SelectionColor::Own`.
    pub header_color: Color32,
    pub inputs: Vec<InputPinLayout>,
    pub outputs: Vec<OutputPinLayout>,
    /// What `NodeContent::splice_pins` said: the pins this node would use if
    /// dropped on a wire, or `None` if it cannot be.
    pub splice_pins: Option<(InputId, OutputId)>,
    /// Off-screen: placed from its last measured geometry, content not built.
    pub culled: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct InputPinLayout {
    pub id: InputId,
    /// The drawn pin, not the grab area.
    pub rect: Rect,
    pub color: Color32,
    pub policy: ConnectionPolicy,
}

#[derive(Clone, Copy, Debug)]
pub struct OutputPinLayout {
    pub id: OutputId,
    /// The drawn pin, not the grab area.
    pub rect: Rect,
    pub color: Color32,
}

#[derive(Clone, Debug)]
pub struct WireLayout {
    pub wire: Wire,
    /// Its source output pin's, so a wire reads as carrying what that output
    /// produces.
    /// The output pin's centre, where the wire starts.
    pub from: Pos2,
    /// The input pin's centre, where it ends.
    pub to: Pos2,
    /// Corners already rounded. The one polyline that was drawn, hit-tested
    /// and cut.
    pub polyline: Vec<Pos2>,
    pub color: Color32,
}

impl Default for GraphLayout {
    fn default() -> Self {
        Self {
            to_global: TSTransform::IDENTITY,
            viewport: Rect::NOTHING,
            nodes: BTreeMap::new(),
            draw_order: Vec::new(),
            wires: Vec::new(),
        }
    }
}

impl NodeLayout {
    /// Every pin with the rect it was drawn in.
    pub fn pins(&self, node: NodeId) -> impl Iterator<Item = (AnyPin, Rect)> + '_ {
        let inputs = self.inputs.iter().map(move |pin| {
            (
                AnyPin::In(InPin {
                    node,
                    input: pin.id,
                }),
                pin.rect,
            )
        });
        let outputs = self.outputs.iter().map(move |pin| {
            (
                AnyPin::Out(OutPin {
                    node,
                    output: pin.id,
                }),
                pin.rect,
            )
        });
        inputs.chain(outputs)
    }

    /// What colour a pin of this node was drawn in.
    pub fn pin_color(&self, pin: AnyPin) -> Option<Color32> {
        match pin {
            AnyPin::In(pin) => self
                .inputs
                .iter()
                .find(|layout| layout.id == pin.input)
                .map(|layout| layout.color),
            AnyPin::Out(pin) => self
                .outputs
                .iter()
                .find(|layout| layout.id == pin.output)
                .map(|layout| layout.color),
        }
    }

    /// The same geometry moved by `offset`.
    pub(super) fn translated(&self, offset: Vec2, culled: bool) -> NodeLayout {
        NodeLayout {
            rect: self.rect.translate(offset),
            header_color: self.header_color,
            inputs: self
                .inputs
                .iter()
                .map(|pin| InputPinLayout {
                    rect: pin.rect.translate(offset),
                    ..*pin
                })
                .collect(),
            outputs: self
                .outputs
                .iter()
                .map(|pin| OutputPinLayout {
                    rect: pin.rect.translate(offset),
                    ..*pin
                })
                .collect(),
            splice_pins: self.splice_pins,
            culled,
        }
    }
}

impl WireLayout {
    /// Where the wire's `+` sits: the middle of its longest run, which on a
    /// forward wire is the horizontal bus into the target.
    pub fn insert_point(&self) -> Option<Pos2> {
        self.polyline
            .windows(2)
            .max_by(|a, b| a[0].distance_sq(a[1]).total_cmp(&b[0].distance_sq(b[1])))
            .map(|segment| segment[0].lerp(segment[1], 0.5))
    }

    pub fn insert_button_rect(&self, size: f32) -> Option<Rect> {
        self.insert_point()
            .map(|center| Rect::from_center_size(center, Vec2::splat(size)))
    }
}

impl GraphLayout {
    /// The topmost node under a graph-space point.
    pub fn node_at(&self, point: Pos2) -> Option<NodeId> {
        self.draw_order.iter().rev().copied().find(|id| {
            self.nodes
                .get(id)
                .is_some_and(|node| node.rect.contains(point))
        })
    }

    /// The pin whose grab area (its rect grown by `slack`) contains the point.
    /// Nearest wins when grab areas overlap.
    pub fn pin_at(&self, point: Pos2, slack: f32) -> Option<AnyPin> {
        self.nodes
            .iter()
            .flat_map(|(id, node)| node.pins(*id))
            .filter(|(_, rect)| rect.expand(slack).contains(point))
            .min_by(|(_, a), (_, b)| {
                a.center()
                    .distance(point)
                    .total_cmp(&b.center().distance(point))
            })
            .map(|(pin, _)| pin)
    }

    /// The wire passing within `slack` of the point. Nearest wins.
    pub fn wire_at(&self, point: Pos2, slack: f32) -> Option<Wire> {
        self.wires
            .iter()
            .map(|wire| (distance_to_polyline(&wire.polyline, point), wire.wire))
            .filter(|(distance, _)| *distance <= slack)
            .min_by(|(a, _), (b, _)| a.total_cmp(b))
            .map(|(_, wire)| wire)
    }

    /// The wire whose `+` button (of the given size) contains the point.
    pub fn wire_with_insert_button_at(&self, point: Pos2, button_size: f32) -> Option<Wire> {
        self.wires
            .iter()
            .find(|wire| {
                wire.insert_button_rect(button_size)
                    .is_some_and(|rect| rect.contains(point))
            })
            .map(|wire| wire.wire)
    }

    pub fn nodes_intersecting(&self, rect: Rect) -> impl Iterator<Item = NodeId> + '_ {
        self.nodes
            .iter()
            .filter(move |(_, node)| node.rect.intersects(rect))
            .map(|(id, _)| *id)
    }

    /// Every node and wire the rect touches - what a box selection over it
    /// selects.
    pub fn contents_of(&self, rect: Rect) -> Selection {
        Selection {
            nodes: self.nodes_intersecting(rect).collect(),
            wires: self
                .wires
                .iter()
                .filter(|wire| polyline_intersects_rect(&wire.polyline, rect))
                .map(|wire| wire.wire)
                .collect(),
        }
    }

    pub fn wire(&self, wire: Wire) -> Option<&WireLayout> {
        self.wires.iter().find(|drawn| drawn.wire == wire)
    }

    pub fn input(&self, pin: InPin) -> Option<&InputPinLayout> {
        self.nodes
            .get(&pin.node)?
            .inputs
            .iter()
            .find(|layout| layout.id == pin.input)
    }

    pub fn output(&self, pin: OutPin) -> Option<&OutputPinLayout> {
        self.nodes
            .get(&pin.node)?
            .outputs
            .iter()
            .find(|layout| layout.id == pin.output)
    }

    pub fn pin_rect(&self, pin: AnyPin) -> Option<Rect> {
        match pin {
            AnyPin::In(pin) => self.input(pin).map(|layout| layout.rect),
            AnyPin::Out(pin) => self.output(pin).map(|layout| layout.rect),
        }
    }

    pub fn pin_color(&self, pin: AnyPin) -> Option<Color32> {
        self.nodes.get(&pin.node())?.pin_color(pin)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{pos2, vec2};

    fn node(rect: Rect) -> NodeLayout {
        NodeLayout {
            rect,
            header_color: Color32::WHITE,
            inputs: vec![],
            outputs: vec![],
            splice_pins: None,
            culled: false,
        }
    }

    fn layout_with(nodes: Vec<(NodeId, NodeLayout)>) -> GraphLayout {
        let draw_order = nodes.iter().map(|(id, _)| *id).collect();
        GraphLayout {
            nodes: nodes.into_iter().collect(),
            draw_order,
            ..Default::default()
        }
    }

    #[test]
    fn the_node_drawn_last_wins_where_two_overlap() {
        let (below, above) = (NodeId(1), NodeId(2));
        let layout = layout_with(vec![
            (
                below,
                node(Rect::from_min_size(pos2(0.0, 0.0), vec2(100.0, 100.0))),
            ),
            (
                above,
                node(Rect::from_min_size(pos2(50.0, 50.0), vec2(100.0, 100.0))),
            ),
        ]);
        assert_eq!(layout.node_at(pos2(75.0, 75.0)), Some(above));
        assert_eq!(layout.node_at(pos2(25.0, 25.0)), Some(below));
        assert_eq!(layout.node_at(pos2(500.0, 500.0)), None);
    }

    #[test]
    fn a_box_selects_the_wires_it_touches_along_with_the_nodes() {
        let wire_layout = |index: u64, polyline: Vec<Pos2>| WireLayout {
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
        };
        let mut layout = layout_with(vec![(
            NodeId(7),
            node(Rect::from_min_size(pos2(55.0, 100.0), vec2(20.0, 20.0))),
        )]);
        layout.wires = vec![
            wire_layout(0, vec![pos2(0.0, 10.0), pos2(200.0, 10.0)]),
            wire_layout(1, vec![pos2(0.0, 500.0), pos2(200.0, 500.0)]),
        ];

        let selection = layout.contents_of(Rect::from_min_max(pos2(50.0, 0.0), pos2(60.0, 110.0)));

        assert_eq!(selection.wires.len(), 1);
        assert!(selection.wires.contains(&layout.wires[0].wire));
        assert!(selection.nodes.contains(&NodeId(7)));
    }

    #[test]
    fn a_pin_is_grabbed_within_its_slack_and_the_nearest_wins() {
        let id = NodeId(1);
        let mut node = node(Rect::from_min_size(pos2(0.0, 0.0), vec2(100.0, 100.0)));
        node.inputs = vec![
            InputPinLayout {
                id: InputId(0),
                rect: Rect::from_center_size(pos2(0.0, 20.0), vec2(8.0, 8.0)),
                color: Color32::WHITE,
                policy: ConnectionPolicy::Single,
            },
            InputPinLayout {
                id: InputId(1),
                rect: Rect::from_center_size(pos2(0.0, 30.0), vec2(8.0, 8.0)),
                color: Color32::WHITE,
                policy: ConnectionPolicy::Single,
            },
        ];
        let layout = layout_with(vec![(id, node)]);

        let second = AnyPin::In(InPin {
            node: id,
            input: InputId(1),
        });
        assert_eq!(layout.pin_at(pos2(0.0, 27.0), 6.0), Some(second));
        assert_eq!(layout.pin_at(pos2(0.0, 27.0), 0.0), Some(second));
        assert_eq!(layout.pin_at(pos2(0.0, 60.0), 6.0), None);
    }
}
