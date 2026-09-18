//! The standalone demo: proves the crate builds and runs without any host
//! application, and is where the canvas is exercised by hand.
//!
//! `cargo run --example demo`

use egui::{Color32, pos2};
use egui_noodle::{
    AnyPin, Canvas, CanvasEvent, CanvasStyle, ConnectionPolicy, Graph, InPin, InputId, InputSpec,
    NodeContent, NodeId, OutPin, OutputId, OutputSpec,
};

fn main() -> eframe::Result {
    eframe::run_native(
        "egui-noodle demo",
        eframe::NativeOptions::default(),
        Box::new(|_| Ok(Box::new(Demo::new()))),
    )
}

struct DemoNode {
    title: String,
    inputs: Vec<&'static str>,
    outputs: Vec<&'static str>,
}

struct DemoContent;

impl NodeContent for DemoContent {
    type Node = DemoNode;

    fn inputs(&mut self, node: &DemoNode) -> Vec<InputSpec> {
        node.inputs
            .iter()
            .enumerate()
            .map(|(index, label)| InputSpec {
                id: InputId(index as u64),
                label: label.to_string(),
                color: pin_color(label),
                policy: ConnectionPolicy::Single,
            })
            .collect()
    }

    fn outputs(&mut self, node: &DemoNode) -> Vec<OutputSpec> {
        node.outputs
            .iter()
            .enumerate()
            .map(|(index, label)| OutputSpec {
                id: OutputId(index as u64),
                label: label.to_string(),
                color: pin_color(label),
            })
            .collect()
    }

    fn title(&mut self, node: &DemoNode) -> String {
        node.title.clone()
    }
}

fn pin_color(label: &str) -> Color32 {
    match label {
        "image" | "in" | "out" => Color32::from_rgb(0xb0, 0x00, 0xb0),
        "amount" | "mix" => Color32::from_rgb(0xb0, 0x00, 0x00),
        _ => Color32::from_rgb(0x00, 0x70, 0xb0),
    }
}

struct Demo {
    graph: Graph<DemoNode>,
    canvas: Canvas,
    style: CanvasStyle,
    content: DemoContent,
    stress_test: bool,
}

impl Demo {
    fn new() -> Self {
        Self {
            graph: small_graph(),
            canvas: Canvas::new(),
            style: CanvasStyle::default(),
            content: DemoContent,
            stress_test: false,
        }
    }
}

impl eframe::App for Demo {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let response = self
            .canvas
            .show(ui, &mut self.graph, &mut self.content, &self.style);
        let stats = response.stats;
        for event in &response.events {
            match event {
                CanvasEvent::ContextMenuRequested { pos } => {
                    let id = self.graph.add_node(new_node(), *pos);
                    self.canvas.select_only(id);
                }
                // What an application's node menu would do: splice a new node
                // into the wire, then let the nodes downstream make room.
                CanvasEvent::WireInsertRequested { wire, pos } => {
                    let size = self.style.default_node_size;
                    let id = self.graph.add_node(new_node(), *pos - size / 2.0);
                    let _ = self.graph.insert_into_wire(
                        *wire,
                        id,
                        InputId(0),
                        ConnectionPolicy::Single,
                        OutputId(0),
                    );
                    self.canvas.make_room(id, wire.to.node);
                    self.canvas.select_only(id);
                }
                CanvasEvent::NodeDroppedOnWire { wire, node, .. } => {
                    self.graph.apply(event);
                    self.canvas.make_room(*node, wire.to.node);
                }
                // What an application's node menu would do: complete the wire
                // with a new node.
                CanvasEvent::WireDropped { from, pos } => {
                    let id = self.graph.add_node(new_node(), *pos);
                    let (from, to) = match from {
                        AnyPin::Out(from) => (*from, in_pin(id, 0)),
                        AnyPin::In(to) => (out_pin(id, 0), *to),
                    };
                    let _ = self.graph.connect(from, to, ConnectionPolicy::Single);
                    self.canvas.select_only(id);
                }
                other => self.graph.apply(other),
            }
        }

        egui::Window::new("Debug")
            .default_pos(pos2(10.0, 10.0))
            .show(&ctx, |ui| {
                if ui.checkbox(&mut self.stress_test, "300 nodes").changed() {
                    self.graph = if self.stress_test {
                        stress_graph()
                    } else {
                        small_graph()
                    };
                }
                let frame_time = ctx.input(|input| input.stable_dt);
                ui.label(format!(
                    "{:.1} ms / frame ({:.0} fps)",
                    frame_time * 1000.0,
                    1.0 / frame_time.max(1e-6)
                ));
                ui.label(format!(
                    "{} nodes, {} culled, {} wires",
                    stats.nodes, stats.culled, stats.wires
                ));
                ui.label(format!("zoom {:.2}", self.canvas.view.zoom));
                ui.label(format!("{} selected", self.canvas.selection().len()));
                ui.checkbox(&mut self.style.snap_to_grid, "snap to grid");
            });
        ctx.request_repaint();
    }
}

/// What the demo adds on a right-click or to complete a wire.
fn new_node() -> DemoNode {
    node("New", &["in", "amount"], &["out"])
}

fn node(title: &str, inputs: &[&'static str], outputs: &[&'static str]) -> DemoNode {
    DemoNode {
        title: title.to_owned(),
        inputs: inputs.to_vec(),
        outputs: outputs.to_vec(),
    }
}

fn small_graph() -> Graph<DemoNode> {
    let mut graph = Graph::new();
    let noise = graph.add_node(
        node("Noise", &["scale", "speed"], &["image"]),
        pos2(0.0, 0.0),
    );
    let blur = graph.add_node(
        node("Blur", &["image", "amount"], &["image"]),
        pos2(220.0, 40.0),
    );
    let gradient = graph.add_node(node("Gradient", &[], &["image"]), pos2(0.0, 160.0));
    let mix = graph.add_node(
        node("Mix", &["image", "image 2", "mix"], &["image"]),
        pos2(440.0, 80.0),
    );
    let feedback = graph.add_node(node("Feedback", &["image"], &["image"]), pos2(220.0, 260.0));
    connect(&mut graph, noise, 0, blur, 0);
    connect(&mut graph, blur, 0, mix, 0);
    connect(&mut graph, gradient, 0, mix, 1);
    connect(&mut graph, mix, 0, feedback, 0);
    // A wire going back left, to see the detour route.
    connect(&mut graph, feedback, 0, blur, 1);
    graph
}

fn stress_graph() -> Graph<DemoNode> {
    let mut graph = Graph::new();
    let mut previous_row: Vec<NodeId> = Vec::new();
    for column in 0..20 {
        let mut row = Vec::new();
        for line in 0..15 {
            let id = graph.add_node(
                node(
                    &format!("Node {column}-{line}"),
                    &["in", "amount"],
                    &["out"],
                ),
                pos2(column as f32 * 240.0, line as f32 * 110.0),
            );
            if let Some(upstream) = previous_row.get(line) {
                connect(&mut graph, *upstream, 0, id, 0);
            }
            row.push(id);
        }
        previous_row = row;
    }
    graph
}

fn connect(graph: &mut Graph<DemoNode>, from: NodeId, output: u64, to: NodeId, input: u64) {
    graph
        .connect(
            out_pin(from, output),
            in_pin(to, input),
            ConnectionPolicy::Single,
        )
        .expect("demo nodes exist");
}

fn out_pin(node: NodeId, output: u64) -> OutPin {
    OutPin {
        node,
        output: OutputId(output),
    }
}

fn in_pin(node: NodeId, input: u64) -> InPin {
    InPin {
        node,
        input: InputId(input),
    }
}
