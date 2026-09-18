# Orthogonal Wires

*2026 September 18*

*Status: implemented.*

Wires are horizontal, vertical, horizontal, with rounded corners and no arrowheads. One polyline per wire is drawn, hit-tested and cut.

## Alternatives considered

**Orthogonal, vs. bezier.** Every egui node library draws beziers, and the first plan did too - it even meant to port snarl's tuned bezier module. The user's preference was orthogonal for readability: wires read as buses, corners are where turns happen, and clutter is lower. It also dissolved the complaint that started the wire work - visible straight segments in snarl's curves. Snarl samples its beziers uniformly in the parameter with a cap of a hundred points, and a node-editor wire (long horizontal tangents) bunches those samples at the ends and starves the middle, which is exactly where the facets showed. A straight segment has no tessellation error at all; only the corner arcs are sampled, to a screen-pixel tolerance divided by the zoom, so they are smooth at any zoom.

**One style, not four.** Snarl offers `Line`, `AxisAligned`, `Bezier3`, `Bezier5`. We draw the one we chose; the routing knobs (`WireRouting`: stub, corner radius, backward clearance) are the styling surface.

**Where the vertical sits: one stub after the source, vs. mid-gap.** A forward wire leaves its output, runs one stub, turns, runs vertically to the target's row, and runs into the target. Putting the vertical near the source means a node's inputs read as a horizontal bus coming in; mid-gap is the classic alternative and spreads sibling verticals apart. Chosen from the user's reference image; it is one small function, isolated to be retuned visually. When the nodes are closer than two stubs the vertical falls to mid-gap so it cannot overshoot the target. Sibling wires leaving one node can share a vertical; a `lane_offset` parameter exists for spreading them and is not yet assigned by the layout.

**Backward wires get their own route.** When the target is left of the source, horizontal-vertical-horizontal would run the vertical back through both nodes. Instead: stub out, vertical to a clearance lane above *or below* both nodes - whichever is nearer to the pins - horizontal back past them, vertical to the target's row, stub in. Choosing the nearer side is what keeps a feedback wire from cutting across the nodes it connects.

**One polyline for drawing and hit-testing, vs. hit-testing the corner points only.** Testing the un-rounded route is cheaper and was briefly agreed to. It was reversed by the user: as the corner radius grows the two shapes diverge, so what the pointer hits would stop matching what the eye sees, and radius is a styling knob we may well turn up. The drawn polyline is cached and is the one thing hit-tested and cut; a pointer against a few hundred short polylines per frame does not need the shortcut. This is also what snarl gets wrong, keeping two independent samplers for draw and hit.

**The `+` sits on the wire's longest run.** The insert button's anchor is the midpoint of the longest segment, which on a forward wire is the bus into the target - so the button lands where the wire is most visibly *that* wire rather than at a corner.
