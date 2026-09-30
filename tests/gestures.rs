//! The core gestures, driven with real pointer input through a headless egui,
//! checked on the graph the emitted events build. Kept to the few gestures
//! whose outcome is settled, whatever the canvas ends up looking like.

use egui::{Color32, Pos2, Vec2};
use egui_kittest::Harness;
use egui_noodle::{
    Canvas, CanvasEvent, CanvasStyle, ConnectionPolicy, Graph, InPin, InputId, InputSpec,
    MovePhase, NodeContent, OutPin, OutputId, OutputSpec, Wire,
};

/// Every node has one input and one output.
struct Content;

impl NodeContent for Content {
    type Node = ();
    type Id = u32;

    fn inputs(&mut self, _node: &()) -> Vec<InputSpec> {
        vec![InputSpec {
            id: InputId(0),
            label: "in".to_owned(),
            color: Color32::GRAY,
            policy: ConnectionPolicy::Single,
        }]
    }

    fn outputs(&mut self, _node: &()) -> Vec<OutputSpec> {
        vec![OutputSpec {
            id: OutputId(0),
            label: "out".to_owned(),
            color: Color32::GRAY,
        }]
    }

    fn title(&mut self, _node: &()) -> String {
        "Node".to_owned()
    }
}

struct App {
    graph: Graph<(), u32>,
    canvas: Canvas<u32>,
    events: Vec<CanvasEvent<u32>>,
}

const LEFT: u32 = 0;
const RIGHT: u32 = 1;

/// Two unconnected nodes side by side, drawn once so the layout exists.
fn harness() -> Harness<'static, App> {
    let mut graph = Graph::new();
    graph.add_node(LEFT, (), Pos2::new(-250.0, -50.0)).unwrap();
    graph.add_node(RIGHT, (), Pos2::new(100.0, -50.0)).unwrap();
    let mut harness = Harness::new_ui_state(
        |ui, app: &mut App| {
            let response = app
                .canvas
                .show(ui, &app.graph, &mut Content, &CanvasStyle::default());
            for event in &response.events {
                app.graph.apply(event);
            }
            app.events.extend(response.events);
        },
        App {
            graph,
            canvas: Canvas::new(),
            events: Vec::new(),
        },
    );
    harness.run();
    harness
}

/// Graph space to where it is on screen.
fn on_screen(harness: &Harness<App>, graph_pos: Pos2) -> Pos2 {
    harness.state().canvas.layout().to_global * graph_pos
}

/// Press at `from`, move to `to` over a few frames, release there.
fn drag(harness: &mut Harness<App>, from: Pos2, to: Pos2) {
    const STEPS: usize = 5;
    harness.hover_at(from);
    harness.step();
    harness.drag_at(from);
    harness.step();
    for step in 1..=STEPS {
        harness.hover_at(from.lerp(to, step as f32 / STEPS as f32));
        harness.step();
    }
    harness.drop_at(to);
    harness.run();
}

#[test]
fn dragging_from_an_output_to_an_input_connects_them() {
    let mut harness = harness();
    let from = OutPin {
        node: LEFT,
        output: OutputId(0),
    };
    let to = InPin {
        node: RIGHT,
        input: InputId(0),
    };
    let layout = harness.state().canvas.layout();
    let output = on_screen(&harness, layout.output(from).unwrap().rect.center());
    let input = on_screen(&harness, layout.input(to).unwrap().rect.center());

    drag(&mut harness, output, input);

    assert_eq!(harness.state().graph.wires(), &[Wire { from, to }]);
}

#[test]
fn dragging_a_node_by_its_header_moves_it_by_the_drag() {
    let mut harness = harness();
    let rect = harness.state().canvas.layout().nodes[&RIGHT].rect;
    let header = rect.center_top() + Vec2::new(0.0, 6.0);
    let start = harness.state().graph.node(RIGHT).unwrap().pos;
    let delta = Vec2::new(60.0, 40.0);

    let grab = on_screen(&harness, header);
    drag(&mut harness, grab, grab + delta);

    let zoom = harness.state().canvas.view.zoom;
    let moved = harness.state().graph.node(RIGHT).unwrap().pos - start;
    assert!((moved - delta / zoom).length() < 0.5, "moved by {moved:?}");
    assert!(harness.state().events.iter().any(|event| matches!(
        event,
        CanvasEvent::NodesMoved {
            phase: MovePhase::Finished,
            ..
        }
    )));
}
