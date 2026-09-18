# Application Owns the Model

*2026 September 18*

*Status: implemented.*

Every node-graph library we looked at (`egui-snarl`, `egui_node_graph2`, `egui-graph-edit`) stores the graph for the application and mutates it in response to input. This crate does the opposite - `Graph` is the application's, `Canvas` is a view over it - and that one inversion is the reason the crate exists. This file records the shape it took and the alternatives it beat.

## Alternatives considered

**The canvas emits events, vs. mutating the graph it is handed.** `Canvas::show` takes `&mut Graph` (node bodies need `&mut` to their payload), so the obvious design is for the canvas to call `set_pos` on drag, `remove_node` on delete, `connect` on drop. Rejected: an undo layer would then have to hook every mutation *inside* the canvas, which is exactly snarl's problem restated. Instead the canvas returns `CanvasEvent`s - `NodesMoved`, `DeleteRequested`, `ConnectRequested`, `NodeDroppedOnWire`, ... - and the application applies them, with `Graph::apply` as the one-line default. The price is the classic immediate-mode one-frame lag on drag, paid for with a **transient offset**: the canvas draws dragged nodes at their provisional positions and emits one `NodesMoved` on release. That gives "one undo entry per drag" by construction, and an application that declines the move sees the node snap back, which is the right affordance for free. The same overlay carries the slide-apart animation after an insert (`make_room`), so no animation frame ever reaches the model or an undo stack.

**Content is content-only.** `NodeContent` receives `&mut Self::Node` and never `&mut Graph`, so nothing a node body does can invalidate the layout mid-frame. Graph-changing intentions raised from inside a node go out as events like everything else. Snarl hands `&mut Snarl<T>` to every viewer method and then re-validates node existence after each call; not offering the reference is simpler than defending against it.

**`make_room` as an explicit call, vs. the canvas inferring it.** When a node is spliced into a wire, the nodes downstream should slide right to make room (Blender's Auto-Offset). The canvas cannot know an insert *succeeded* - the application applies the event - so inferring the slide from events would animate a rejected insert. Instead the application calls `canvas.make_room(inserted, downstream)` after applying, and the canvas resolves it on the first frame the inserted node has a measured rect, which also covers the node the application creates from a wire's `+` (it does not exist at click time). One call, explicit, application in control.

**Selection is view state, not model state.** Blender's selection is undoable; ours is not. Each future view over the same graph - a layer-stack view, a nested subgraph - wants its own selection, so putting it in the model would mean `HashMap<ViewId, Selection>` and a model that knows about views. Settable from outside (`select_only`, `set_selection`), which snarl never allowed.

**`ConnectionPolicy` per connect, vs. baked into the graph.** The graph allows any number of wires into an input; the application declares `Single` or `Multiple` per input through `InputSpec`, and the canvas passes it along with each `ConnectRequested`. Coollab wants one wire per input everywhere; feedback zones and struct-like pins in its own design notes want several. Generality lives in what the crate *allows*, never in what a given application's graph contains.
