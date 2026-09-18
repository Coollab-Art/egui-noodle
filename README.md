# egui-noodle

A node-graph canvas for [egui](https://github.com/emilk/egui): nodes holding arbitrary egui content, pins, orthogonal wires, pan and zoom, selection, and the gestures that make a node editor comfortable to use.

**Status: early development.** Built for [Coollab](https://coollab-art.com) and driven by its needs first. The API will change.

## Philosophy: you own the model

Most node-graph libraries store the graph for you: you hand them nodes, they keep positions and wires internally, and you read the result back out. egui-noodle does the opposite. **Your application owns the `Graph`** - nodes, positions, wires - and the canvas is a pure view over it that reports what the user did.

This is how Blender's node editor works, and it is why an addon like Node Wrangler can add dozens of gestures without touching the editor's code: the graph is plain data the addon mutates freely, and the editor publishes enough geometry (node rects, socket positions) to build any interaction on top.

**What you get:**

- Every change to the graph goes through *your* code, so undo/redo, commands, scripting, remote control and collaborative editing are yours to add without fighting the widget.
- Node rects, pin positions and wire paths are published every frame, so hit-testing and custom gestures are ordinary application code.
- Node ids are never recycled, so a stale id can never silently address a different node - safe to serialize and to hold across deletions.
- Animations (nodes sliding apart to make room for an insert) live in the view, never in your data.

**What it costs:**

- There is no built-in undo. You build it, on a mutation API that is shaped for it: every mutation returns what its inverse needs.
- You hold the graph and hand it to the canvas each frame; the canvas keeps only view state (pan, zoom, selection).
- Gestures read the previous frame's geometry - the same way egui itself hit-tests - so a gesture starting on the very first frame a node appears sees nothing there yet.
- The canvas rebuilds a geometry cache every frame. It is cheap for hundreds of nodes and culls what is off-screen, but it is a per-frame cost that a library owning its own layout could amortize differently.

## Documentation

- [Shortcuts](docs/shortcuts.md) - every gesture and key binding.
- [Architecture](docs/architecture.md) - how the crate is put together.
- [Post-mortems](post-mortems/) - the decisions, and why the alternatives lost.

## License

Boost Software License 1.0 - see [LICENSE](LICENSE).
