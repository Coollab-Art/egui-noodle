use super::distance_to_polyline;
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
    /// Screen space: the area the canvas was drawn into.
    pub panel_rect: Rect,
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
    pub header_rect: Rect,
    pub inputs: Vec<InputPinLayout>,
    pub outputs: Vec<OutputPinLayout>,
    /// Off-screen: laid out from its last measured size, content not built.
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
    /// Corners already rounded. The one polyline that was drawn, hit-tested
    /// and cut.
    pub polyline: Vec<Pos2>,
    pub color: Color32,
}

impl Default for GraphLayout {
    fn default() -> Self {
        Self {
            to_global: TSTransform::IDENTITY,
            panel_rect: Rect::NOTHING,
            viewport: Rect::NOTHING,
            nodes: BTreeMap::new(),
            draw_order: Vec::new(),
            wires: Vec::new(),
        }
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
        let mut best: Option<(f32, AnyPin)> = None;
        let mut consider = |pin: AnyPin, rect: Rect| {
            if !rect.expand(slack).contains(point) {
                return;
            }
            let distance = rect.center().distance(point);
            if best.is_none_or(|(best_distance, _)| distance < best_distance) {
                best = Some((distance, pin));
            }
        };
        for (id, node) in &self.nodes {
            for pin in &node.inputs {
                consider(
                    AnyPin::In(InPin {
                        node: *id,
                        input: pin.id,
                    }),
                    pin.rect,
                );
            }
            for pin in &node.outputs {
                consider(
                    AnyPin::Out(OutPin {
                        node: *id,
                        output: pin.id,
                    }),
                    pin.rect,
                );
            }
        }
        best.map(|(_, pin)| pin)
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

    pub fn nodes_intersecting(&self, rect: Rect) -> impl Iterator<Item = NodeId> + '_ {
        self.nodes
            .iter()
            .filter(move |(_, node)| node.rect.intersects(rect))
            .map(|(id, _)| *id)
    }

    pub fn nodes_contained_by(&self, rect: Rect) -> impl Iterator<Item = NodeId> + '_ {
        self.nodes
            .iter()
            .filter(move |(_, node)| rect.contains_rect(node.rect))
            .map(|(id, _)| *id)
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
        match pin {
            AnyPin::In(pin) => self.input(pin).map(|layout| layout.color),
            AnyPin::Out(pin) => self.output(pin).map(|layout| layout.color),
        }
    }
}

impl InputPinLayout {
    pub(super) fn translated(&self, offset: Vec2) -> Self {
        Self {
            rect: self.rect.translate(offset),
            ..*self
        }
    }
}

impl OutputPinLayout {
    pub(super) fn translated(&self, offset: Vec2) -> Self {
        Self {
            rect: self.rect.translate(offset),
            ..*self
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{pos2, vec2};

    fn node(rect: Rect) -> NodeLayout {
        NodeLayout {
            rect,
            header_rect: Rect::NOTHING,
            inputs: vec![],
            outputs: vec![],
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
