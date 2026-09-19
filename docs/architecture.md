# Architecture

Three layers with one-directional dependencies:

```
model     - the Graph: nodes, positions, wires. No egui interaction, only emath types.
   | read
layout    - per-frame geometry: node rects, pin rects, wire polylines. Graph space.
   | read
gestures + rendering - the only code that touches egui input.
```

**Gestures read the previous frame's layout.** egui itself hit-tests against the previous pass's widget rects, so this is in phase with the framework rather than a compromise, and it removes any need to draw a node, discover it moved, and patch up within one frame.

**The canvas reports; the application decides.** `Canvas::show` returns `CanvasEvent`s - a node was dropped here, a wire was released there, the `+` on a wire was clicked - and never changes the graph. `Graph::apply` is the plain response for applications without undo; one with undo turns each event into a command instead.

## Model (`src/model/`)

- `Graph<N>` is a `BTreeMap<NodeId, Node<N>>` plus a `Vec<Wire>`. Both iterate in creation order, so nothing the application derives from the graph depends on hash order.
- `NodeId(u64)` is minted monotonically and **never recycled**, so a stale id can never address a different node. `add_node_with_id` restores one, for undo and for loading a document.
- `InputId` and `OutputId` are opaque and application-supplied; the graph stores and compares them and never interprets them. A wire *is* its `(OutPin, InPin)` pair and has no id of its own.
- Every mutation returns what its inverse needs (`RemovedNode`, `Connected::New { displaced }`, `Inserted`) and none panics on a stale id. That is the seam an undo layer plugs into.
- `ConnectionPolicy` is passed to each `connect`: the graph itself allows any number of wires into an input, and enforces `Single` by displacing when asked.

## Canvas (`src/canvas/`)

`Canvas` owns view state only: `ViewState { center, zoom }` in graph terms (the screen transform is derived from the panel each frame, so moving or resizing the panel changes nothing), the selection (nodes and wires), the gesture in progress, each node's last measured size and pin placement, last frame's `GraphLayout`, and the transient offsets of a drag or a slide-apart animation.

One frame of `Canvas::show`, in order:

1. Three sublayers with `set_transform_layer`, so every rect below is in graph space and egui inverts the transform when it hit-tests: the base (grid, wires, the background response), the nodes, and the overlay. The nodes layer is scaled by `content_scale` - the zoom, when zoomed in - and its transform divided by the same, so node content is laid out at the size it is shown at and text is rasterised crisp instead of magnified. `NodeContent` sees that scaled `Ui`, with its style pre-scaled to match.
2. Interactors, from **last frame's** layout: the background on the base layer; node frames then pins on the nodes layer, scaled like it; then the `+` on a hovered wire. Later registration wins egui's tie-break, so a widget inside a node beats the node's frame and a pin beats the node's edge. Under the command modifier frames sense clicks only, so Ctrl+drag falls through to the background - that is the cut stroke.
3. Hover, then the gesture state machine (`gestures.rs` holds the state, `input.rs` drives it), which emits events. Then the slide-apart animation, if one is running.
4. Grid; a paint slot reserved for the wires, which go behind nodes but need this frame's pin positions.
5. Nodes (`draw.rs`), back to front, each followed immediately by its own marks - its selection outline, then its pins over that outline. Drawing them with the node rather than on the overlay is what makes a node in front cover the pins of one behind it. A node off-screen is placed from its last measure without building its content. A node never measured is laid out against `default_node_size` in a sizing pass and the frame is discarded, as egui's `Area` does; after that, against its measured size, with text set to extend rather than wrap so the node can grow. The geometry comes back in graph units.
6. Wires routed from this frame's pins (`wires.rs`): an orthogonal route, corners rounded to a screen-pixel tolerance, one polyline that is drawn, hit-tested, box-selected and cut. Highlighted wires are marked here, behind the nodes: a selected wire is outlined by a wider stroke behind it, one hovered or about to be cut is restroked over.
7. The overlay, above every node: the box, snap guides, the wire being dragged and the pin it would land on, the cutting stroke, the `+` on a hovered wire.
8. This frame's layout becomes next frame's. Nodes just dropped keep being drawn where they landed until the graph, which learns of the move after `show` returns, catches up.

`NodeContent` is content-only - it never sees the graph - so nothing it draws can invalidate the layout mid-frame. Its one structural hook is `splice_pins`, which says which pins a node uses when dropped on a wire, and which wire a chain closes over when the node is deleted (`Graph::bridges_over`).

Animations (`make_room`) are view-only offsets on top of the graph's positions; one `NodesMoved` reports where the nodes ended up once the slide settles.
