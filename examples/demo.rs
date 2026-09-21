//! The standalone demo: proves the crate builds and runs without any host
//! application, and is where the canvas is exercised by hand.
//!
//! `cargo run --example demo`

use egui::{Color32, Rect, pos2};
use egui_noodle::{
    AnyPin, Canvas, CanvasEvent, CanvasStyle, ConnectionPolicy, Graph, InPin, InputId, InputSpec,
    NodeContent, OutPin, OutputId, OutputSpec, SequentialNodeId, Wire,
};

fn main() -> eframe::Result {
    eframe::run_native(
        "egui-noodle demo",
        eframe::NativeOptions::default(),
        Box::new(|_| Ok(Box::new(Demo::new()))),
    )
}

/// The demo does not care how nodes are identified, so it takes the
/// ready-made sequential id and mints from a counter.
type NodeId = SequentialNodeId;

struct DemoNode {
    title: String,
    inputs: Vec<&'static str>,
    outputs: Vec<&'static str>,
}

struct DemoContent;

impl NodeContent for DemoContent {
    type Node = DemoNode;
    type Id = NodeId;

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

/// A graph plus the counter its ids come from - the application's business,
/// never the graph's.
struct DemoGraph {
    graph: Graph<DemoNode, NodeId>,
    next_id: u64,
}

impl DemoGraph {
    fn new() -> Self {
        Self {
            graph: Graph::new(),
            next_id: 0,
        }
    }

    fn add(&mut self, node: DemoNode, pos: egui::Pos2) -> NodeId {
        let id = SequentialNodeId(self.next_id);
        self.next_id += 1;
        self.graph
            .add_node(id, node, pos)
            .expect("demo ids are never reused");
        id
    }

    fn connect(&mut self, from: NodeId, output: u64, to: NodeId, input: u64) {
        self.graph
            .connect(Wire {
                from: out_pin(from, output),
                to: in_pin(to, input),
            })
            .expect("demo nodes exist");
    }
}

struct Demo {
    document: DemoGraph,
    canvas: Canvas<NodeId>,
    style: CanvasStyle,
    content: DemoContent,
    stress_test: bool,
}

impl Demo {
    fn new() -> Self {
        Self {
            document: small_graph(),
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
            .show(ui, &self.document.graph, &mut self.content, &self.style);
        let stats = response.stats;
        for event in &response.events {
            match event {
                CanvasEvent::ContextMenuRequested { pos } => {
                    let id = self.document.add(new_node(), *pos);
                    self.canvas.select_only(id);
                }
                // What an application's node menu would do: measure the node
                // it is about to create, make room for it in the wire, then
                // splice it in.
                CanvasEvent::WireInsertRequested { wire, pos } => {
                    let node = new_node();
                    let id = SequentialNodeId(self.document.next_id);
                    let size =
                        self.canvas
                            .measure_node(ui, &mut self.content, id, &node, &self.style);
                    let rect = Rect::from_center_size(*pos, size);
                    let moves =
                        self.canvas
                            .make_room(&self.document.graph, *wire, rect, &self.style);
                    for node_move in moves {
                        self.document.graph.set_pos(node_move.id, node_move.to);
                    }
                    let id = self.document.add(node, rect.min);
                    let _ = self.document.graph.insert_into_wire(
                        *wire,
                        id,
                        InputId(0),
                        ConnectionPolicy::Single,
                        OutputId(0),
                    );
                    self.canvas.select_only(id);
                }
                CanvasEvent::NodeDroppedOnWire { wire, node, .. } => {
                    if let Some(rect) = self.canvas.layout().nodes.get(node).map(|n| n.rect) {
                        let moves =
                            self.canvas
                                .make_room(&self.document.graph, *wire, rect, &self.style);
                        for node_move in moves {
                            self.document.graph.set_pos(node_move.id, node_move.to);
                        }
                    }
                    self.document.graph.apply(event);
                }
                // What an application's node menu would do: complete the wire
                // with a new node.
                CanvasEvent::WireDropped { from, pos } => {
                    let id = self.document.add(new_node(), *pos);
                    let wire = match from {
                        AnyPin::Out(from) => Wire {
                            from: *from,
                            to: in_pin(id, 0),
                        },
                        AnyPin::In(to) => Wire {
                            from: out_pin(id, 0),
                            to: *to,
                        },
                    };
                    self.document
                        .graph
                        .displace(wire.to, ConnectionPolicy::Single);
                    let _ = self.document.graph.connect(wire);
                    self.canvas.select_only(id);
                }
                other => self.document.graph.apply(other),
            }
        }

        egui::Window::new("Debug")
            .default_pos(pos2(10.0, 10.0))
            .show(&ctx, |ui| {
                if ui.checkbox(&mut self.stress_test, "300 nodes").changed() {
                    self.document = if self.stress_test {
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
                ui.label(format!("zoom {:.3}", self.canvas.view.zoom));
                if let Some(id) = self.canvas.selection().nodes.first()
                    && let Some(node) = self.canvas.layout().nodes.get(id)
                {
                    ui.label(format!(
                        "selected node {:.3} x {:.3}",
                        node.rect.width(),
                        node.rect.height()
                    ));
                }
                ui.label(format!(
                    "{} nodes and {} wires selected",
                    self.canvas.selection().nodes.len(),
                    self.canvas.selection().wires.len()
                ));
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

fn small_graph() -> DemoGraph {
    let mut document = DemoGraph::new();
    let noise = document.add(
        node("Noise", &["scale", "speed"], &["image"]),
        pos2(0.0, 0.0),
    );
    let blur = document.add(
        node("Blur", &["image", "amount"], &["image"]),
        pos2(220.0, 40.0),
    );
    let gradient = document.add(node("Gradient", &[], &["image"]), pos2(0.0, 160.0));
    let mix = document.add(
        node("Mix", &["image", "image 2", "mix"], &["image"]),
        pos2(440.0, 80.0),
    );
    let feedback = document.add(node("Feedback", &["image"], &["image"]), pos2(220.0, 260.0));
    document.connect(noise, 0, blur, 0);
    document.connect(blur, 0, mix, 0);
    document.connect(gradient, 0, mix, 1);
    document.connect(mix, 0, feedback, 0);
    // A wire going back left, to see the detour route.
    document.connect(feedback, 0, blur, 1);
    document
}

fn stress_graph() -> DemoGraph {
    let mut document = DemoGraph::new();
    let mut previous_row: Vec<NodeId> = Vec::new();
    for column in 0..20 {
        let mut row = Vec::new();
        for line in 0..15 {
            let id = document.add(
                node(
                    &format!("Node {column}-{line}"),
                    &["in", "amount"],
                    &["out"],
                ),
                pos2(column as f32 * 240.0, line as f32 * 110.0),
            );
            if let Some(upstream) = previous_row.get(line) {
                document.connect(*upstream, 0, id, 0);
            }
            row.push(id);
        }
        previous_row = row;
    }
    document
}

fn out_pin(node: NodeId, output: u64) -> OutPin<NodeId> {
    OutPin {
        node,
        output: OutputId(output),
    }
}

fn in_pin(node: NodeId, input: u64) -> InPin<NodeId> {
    InPin {
        node,
        input: InputId(input),
    }
}
