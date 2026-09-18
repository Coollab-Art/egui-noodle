# Architecture

Three layers with one-directional dependencies:

```
model     - the Graph: nodes, positions, wires. No egui interaction, only emath types.
   | read
layout    - per-frame geometry cache: node rects, pin rects, wire polylines. Graph space.
   | read
gestures + rendering - the only code that touches egui input.
```

**Gestures read the previous frame's layout.** egui itself hit-tests against the previous pass's widget rects, so this is in phase with the framework rather than a compromise, and it removes any need to draw a node, discover it moved, and patch up within one frame.

**The canvas reports; the application decides.** `show()` returns what happened on the canvas - a right-click on empty space at a graph position, a wire dropped in the open, an insert requested on a wire - in the canvas's own vocabulary. What menu to open, what node that means, is the application's business.

*Filled in as the crate is built.*
