use crate::{ConnectionPolicy, InputId, NodeId, OutputId};
use egui::{Color32, Ui};

/// How the application draws its nodes. Content only: it never sees the
/// graph, so nothing it does can invalidate the layout mid-frame. Anything
/// that should change the graph is reported back through the canvas's events
/// and applied by the application afterwards.
///
/// The `Ui` handed to each method is in graph space, inside the node's frame.
pub trait NodeContent {
    /// What the application stores per node - `Graph<Self::Node>`.
    type Node;

    fn inputs(&mut self, node: &Self::Node) -> Vec<InputSpec>;
    fn outputs(&mut self, node: &Self::Node) -> Vec<OutputSpec>;

    fn title(&mut self, node: &Self::Node) -> String;

    /// The header row. Defaults to the title as a non-selectable label - a
    /// selectable one would swallow the drag that moves the node.
    fn header(&mut self, ui: &mut Ui, id: NodeId, node: &mut Self::Node) {
        let _ = id;
        let title = self.title(node);
        ui.add(egui::Label::new(title).selectable(false));
    }

    /// Between the input and output pin columns. Defaults to nothing.
    fn body(&mut self, ui: &mut Ui, id: NodeId, node: &mut Self::Node) {
        let _ = (ui, id, node);
    }

    /// The row next to one input pin. Defaults to the label.
    fn input_row(&mut self, ui: &mut Ui, input: &InputSpec) {
        ui.add(egui::Label::new(&input.label).selectable(false));
    }

    /// The row next to one output pin. Defaults to the label.
    fn output_row(&mut self, ui: &mut Ui, output: &OutputSpec) {
        ui.add(egui::Label::new(&output.label).selectable(false));
    }
}

pub struct InputSpec {
    pub id: InputId,
    pub label: String,
    /// The pin's fill; a wire takes the mix of its two ends' colours.
    pub color: Color32,
    pub policy: ConnectionPolicy,
}

pub struct OutputSpec {
    pub id: OutputId,
    pub label: String,
    pub color: Color32,
}
