# Node and Wire Identity

*2026 September 18*

*Status: implemented.*

How a node, a wire and a pin are named decides whether an undo stack, a saved document and a hot-reloaded definition can hold on to them safely. Snarl's `NodeId` is a recycled slab index with no generation: after a deletion, the next node can take the same index and a stale id silently points at it.

## Alternatives considered

**A never-recycled `u64`, vs. a generational arena plus a separate stable id.** The first design had both: a `NodeId { index, generation }` for fast, stale-safe runtime access, and a `StableId(u64)` per node for serialization. That is one id too many. Minting `NodeId(u64)` monotonically and never reusing it gives the generation guarantee and more - a removed node's id never matches anything again, so there is no stale-handle window at all - it serializes directly, and the arena, the generation counters and the mapping between the two ids all disappear. Storage is a `BTreeMap<NodeId, Node>`: at hundreds of nodes the lookup cost is nothing, and iteration is creation order, so nothing an application derives from the graph depends on hash order. The application can supply an id (`add_node_with_id`) to restore one on undo or on load; ids minted afterwards stay above it.

**No `WireId`.** A wire is a relation, not a thing: `(OutPin, InPin)` is unique by construction, is what the layout, wire-cut, insert-on-drop and undo all address, and is the form the consuming application records anyway. Reversible if per-wire data ever becomes real - a label, a colour override, reroute points - which a `HashMap<(OutPin, InPin), WireData>` would carry; inventing an id for it now would be predesign.

**Opaque, application-supplied pin ids, vs. positional, vs. the display string.** A pin's identity must not be its position in the node's pin list (adding a pin above it moves every wire) and must not be its label (renaming should be free). `InputId`/`OutputId` are opaque `u64`s the graph stores and compares and never interprets; the application decides what they are - a stable hash of a name for pins defined by an external file, a minted counter for pins the application creates and lets the user rename - and the label travels separately in `InputSpec`.

**Wires as an insertion-ordered `Vec`, no reverse indices.** `by_input`/`by_output` maps were planned and dropped: every query scans the wire list, which at a few hundred wires is microseconds even inside a drag, and three structures kept consistent is where model bugs live. Add an index when a profile asks for one.
