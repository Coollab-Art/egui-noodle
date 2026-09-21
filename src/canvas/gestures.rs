//! The gesture in progress - what the user is dragging, selecting or cutting -
//! and what it leaves behind. Reading input and driving the machine lives in
//! `input`; the snapping a node drag goes through lives in `snap`.

use super::{Canvas, CanvasEvent, CanvasStyle, NodeMove, Selection, snap::Guide};
use crate::{AnyPin, Graph, InPin, MovePhase, NodeIdentifier, OutPin, Wire};
use egui::{Modifiers, Pos2, Rect, Response, Vec2};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Default)]
pub(super) enum Gesture<I> {
    #[default]
    Idle,
    DraggingNodes {
        /// The node whose frame the drag started on; its response says when
        /// the drag ends.
        handle: I,
        /// Each dragged node's offset from the pointer, so the group keeps its
        /// shape and the grabbed point stays under the cursor.
        grab: BTreeMap<I, Vec2>,
        /// Where the nodes are drawn this frame, and what the graph is told
        /// every frame the pointer moves.
        targets: BTreeMap<I, Pos2>,
        guide_x: Option<Guide>,
        guide_y: Option<Guide>,
        /// The wire a single dragged node would be spliced into on release.
        insert_target: Option<InsertTarget<I>>,
    },
    BoxSelecting {
        start: Pos2,
        current: Pos2,
        /// What the selection will be on release - shown as selected already,
        /// so the box gives feedback as it grows.
        preview: Selection<I>,
    },
    DraggingWire {
        /// The pin the drag started on. Always a new wire: an input that is
        /// already wired keeps its wire until the new one lands. See
        /// `4-Input Arbitration.md`.
        origin: AnyPin<I>,
        current: Pos2,
    },
    Cutting {
        /// The stroke so far, in graph space.
        stroke: Vec<Pos2>,
        /// Every wire and node the stroke has crossed. Shown as it grows,
        /// removed on release.
        wires: Vec<Wire<I>>,
        nodes: Vec<I>,
    },
}

impl<I: NodeIdentifier> Gesture<I> {
    /// Whether the pointer is dragging something the view should follow to
    /// the panel's edge.
    pub(super) fn is_drag(&self) -> bool {
        match self {
            Gesture::Idle | Gesture::Cutting { .. } => false,
            Gesture::DraggingNodes { .. }
            | Gesture::BoxSelecting { .. }
            | Gesture::DraggingWire { .. } => true,
        }
    }

    /// The nodes the cut stroke has crossed so far, shown as about to go.
    pub(super) fn cut_nodes(&self) -> &[I] {
        match self {
            Gesture::Cutting { nodes, .. } => nodes,
            _ => &[],
        }
    }

    /// The nodes this gesture hangs off. Their interactors are registered
    /// even when they scroll off-screen: a gesture ends when its interactor
    /// is gone, which is meant to catch the node being deleted, so losing one
    /// to culling would end the drag as if the pointer had been released.
    pub(super) fn nodes(&self) -> BTreeSet<I> {
        match self {
            Gesture::Idle | Gesture::BoxSelecting { .. } | Gesture::Cutting { .. } => {
                BTreeSet::new()
            }
            Gesture::DraggingNodes {
                handle, targets, ..
            } => targets
                .keys()
                .copied()
                .chain(std::iter::once(*handle))
                .collect(),
            Gesture::DraggingWire { origin, .. } => std::iter::once(origin.node()).collect(),
        }
    }
}

/// The wire under a dragged node, and whether the node can go into it.
#[derive(Clone, Copy, Debug)]
pub(super) struct InsertTarget<I> {
    pub wire: Wire<I>,
    /// False when the node has no pins to splice with: the wire is shown as
    /// refusing, and releasing does nothing.
    pub valid: bool,
}

/// What is under the pointer while nothing is being dragged.
#[derive(Clone, Copy, Debug)]
pub struct Hovered<I> {
    pub wire: Option<Wire<I>>,
    pub pin: Option<AnyPin<I>>,
}

// By hand: a derive would demand `I: Default`.
impl<I> Default for Hovered<I> {
    fn default() -> Self {
        Self {
            wire: None,
            pin: None,
        }
    }
}

/// The interactors registered for one kind of hit area, with what egui
/// reported for each.
pub(super) type Interactors<K> = Vec<(K, Response)>;

/// What egui reported this frame for the interactors registered from last
/// frame's layout.
#[derive(Clone, Copy)]
pub(super) struct Interaction<'a, I> {
    pub frames: &'a [(I, Response)],
    pub pins: &'a [(AnyPin<I>, Response)],
    pub insert_button: Option<&'a (Rect, Response)>,
    pub background: &'a Response,
    /// Graph space.
    pub pointer: Option<Pos2>,
    pub modifiers: Modifiers,
}

impl<I: NodeIdentifier> Canvas<I> {
    /// What the pointer is over while nothing is being dragged, from where
    /// everything was drawn last frame. A wire's `+` counts as the wire, so
    /// the hover holds while the pointer crosses onto the button.
    pub(super) fn update_hovered(
        &mut self,
        pointer: Option<Pos2>,
        over_canvas: bool,
        style: &CanvasStyle,
    ) {
        self.hovered = Hovered::default();
        if !matches!(self.gesture, Gesture::Idle) || !over_canvas {
            return;
        }
        let Some(pointer) = pointer else {
            return;
        };
        self.hovered.pin = self.layout.pin_at(pointer, style.pin_hit_expansion);
        if self.hovered.pin.is_none() && self.layout.node_at(pointer).is_none() {
            self.hovered.wire = self
                .layout
                .wire_at(pointer, style.wire_hit_slack)
                .or_else(|| {
                    self.layout
                        .wire_with_insert_button_at(pointer, style.insert_button_size)
                });
        }
    }

    /// The selection as it should be drawn: the box's preview while one is
    /// being dragged, the real selection otherwise.
    pub(super) fn shown_selection(&self) -> &Selection<I> {
        match &self.gesture {
            Gesture::BoxSelecting { preview, .. } => preview,
            _ => &self.selection,
        }
    }

    /// A `DeleteRequested` for these nodes and wires: wires touching a
    /// removed node go with it, and the chains the nodes sat in are bridged.
    pub(super) fn delete_request<N>(
        &self,
        graph: &Graph<N, I>,
        nodes: Vec<I>,
        mut wires: Vec<Wire<I>>,
    ) -> CanvasEvent<I> {
        wires.retain(|wire| !nodes.contains(&wire.from.node) && !nodes.contains(&wire.to.node));
        let removed = nodes
            .iter()
            .map(|id| {
                (
                    *id,
                    self.layout.nodes.get(id).and_then(|node| node.splice_pins),
                )
            })
            .collect();
        CanvasEvent::DeleteRequested {
            bridges: graph.bridges_over(&removed),
            nodes,
            wires,
        }
    }

    /// The wire a dragged node is over, when exactly one node is dragged and
    /// the alt modifier is not held down - alt is "move past this wire".
    pub(super) fn insert_target_under(
        &self,
        pointer: Pos2,
        targets: &BTreeMap<I, Pos2>,
        modifiers: Modifiers,
        style: &CanvasStyle,
    ) -> Option<InsertTarget<I>> {
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

    /// One frame of a node drag: a `NodesMoved` naming every dragged node,
    /// whenever any of them is not where the graph has it, and on release
    /// whatever happened. Always the whole dragged set, so an application
    /// grouping the drag into one undo entry sees the same set each frame.
    /// The nodes keep being drawn where they are until the graph catches up.
    /// On `Finished`, the splice follows if the node was dropped on a wire it
    /// can go into.
    pub(super) fn report_moves<N>(
        &mut self,
        graph: &Graph<N, I>,
        targets: &BTreeMap<I, Pos2>,
        phase: MovePhase,
        insert_target: Option<InsertTarget<I>>,
        events: &mut Vec<CanvasEvent<I>>,
    ) {
        let any_moved = targets
            .iter()
            .any(|(id, to)| graph.node(*id).is_some_and(|node| node.pos != *to));
        if any_moved || phase == MovePhase::Finished {
            let moves: Vec<NodeMove<I>> = targets
                .iter()
                .filter(|(id, _)| graph.contains(**id))
                .map(|(id, to)| NodeMove { id: *id, to: *to })
                .collect();
            self.settled
                .extend(moves.iter().map(|node_move| (node_move.id, node_move.to)));
            events.push(CanvasEvent::NodesMoved { moves, phase });
        }
        if phase != MovePhase::Finished {
            return;
        }
        if let Some(InsertTarget { wire, valid: true }) = insert_target
            && let Some((&node, _)) = targets.iter().next()
            && let Some(node_layout) = self.layout.nodes.get(&node)
            && let Some((input, output)) = node_layout.splice_pins
            && let Some(input_layout) = node_layout.inputs.iter().find(|pin| pin.id == input)
        {
            events.push(CanvasEvent::NodeDroppedOnWire {
                wire,
                node,
                input,
                input_policy: input_layout.policy,
                output,
            });
        }
    }

    /// What a wire drag becomes when it is released over `target`: a
    /// connection to a compatible pin, a request for a node when released on
    /// empty canvas, and otherwise nothing. Connecting into a `Single` input
    /// displaces what was there, so dragging a fresh wire out of a wired
    /// input and landing it replaces the old wire.
    pub(super) fn release_wire(
        &self,
        origin: AnyPin<I>,
        target: Option<AnyPin<I>>,
        pos: Pos2,
        events: &mut Vec<CanvasEvent<I>>,
    ) {
        let (from, to) = match (origin, target) {
            (AnyPin::Out(from), Some(AnyPin::In(to)))
            | (AnyPin::In(to), Some(AnyPin::Out(from))) => (from, to),
            (from, None) => {
                events.push(CanvasEvent::WireDropped { from, pos });
                return;
            }
            _ => return,
        };
        if can_connect(from, to)
            && let Some(policy) = self.layout.input(to).map(|input| input.policy)
        {
            events.push(CanvasEvent::ConnectRequested { from, to, policy });
        }
    }

    pub(super) fn begin_node_drag<N>(
        &self,
        graph: &Graph<N, I>,
        handle: I,
        pointer: Pos2,
    ) -> Gesture<I> {
        let targets: BTreeMap<I, Pos2> = self
            .selection
            .nodes
            .iter()
            .filter_map(|id| Some((*id, graph.node(*id)?.pos)))
            .collect();
        let grab = targets
            .iter()
            .map(|(id, pos)| (*id, *pos - pointer))
            .collect();
        Gesture::DraggingNodes {
            handle,
            targets,
            grab,
            guide_x: None,
            guide_y: None,
            insert_target: None,
        }
    }

    /// Where a node is drawn this frame: its provisional position while it is
    /// being dragged, where it was just dropped until the graph has caught
    /// up, its graph position plus any sliding offset otherwise.
    pub(super) fn drawn_pos(&self, id: I, graph_pos: Pos2) -> Pos2 {
        if let Gesture::DraggingNodes { targets, .. } = &self.gesture
            && let Some(target) = targets.get(&id)
        {
            return *target;
        }
        if let Some(settled) = self.settled.get(&id) {
            return *settled;
        }
        graph_pos + self.sliding_offset(id)
    }

    pub(super) fn raise(&mut self, id: I) {
        if let Some(index) = self.draw_order.iter().position(|other| *other == id) {
            self.draw_order.remove(index);
            self.draw_order.push(id);
        }
    }
}

/// The one rule the canvas itself imposes: a node does not wire into itself.
/// Type compatibility is the application's, through the events it accepts.
pub(super) fn can_connect<I: NodeIdentifier>(from: OutPin<I>, to: InPin<I>) -> bool {
    from.node != to.node
}
