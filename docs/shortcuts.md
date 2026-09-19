# Shortcuts and gestures

Every interaction the canvas offers, with its binding. Kept in step with the code: a gesture lands in the same commit as its line here.

## Navigating

| Gesture | Effect |
| --- | --- |
| Middle-drag or right-drag on the canvas | Pan |
| Mouse wheel | Zoom, around the pointer |
| Shift + wheel, Alt + wheel | Pan horizontally / vertically |
| Ctrl + wheel, pinch | Zoom, around the pointer |
| Drag anything to the edge of the panel | Pan in that direction, faster the closer to the edge, and on at full speed while the pointer is held past it |

The primary button is left free for selecting and dragging nodes.

## Selecting

| Gesture | Effect |
| --- | --- |
| Click a node or a wire | Select it alone |
| Ctrl + click a node or a wire (Cmd on macOS) | Add it to or remove it from the selection |
| Drag on empty canvas | Box-select every node and wire the box touches; they show as selected while the box grows |
| Shift + drag on empty canvas | Box-select, adding to the selection |
| Click empty canvas | Deselect all |

## Editing

| Gesture | Effect |
| --- | --- |
| Drag a node | Move it, and every other selected node with it |
| While dragging: hold Shift | Do not snap |
| Delete, Backspace | Delete the selected nodes and wires; a chain closes over a deleted node (`A -> B -> C` less `B` becomes `A -> C`) |
| Right-click empty canvas | Ask the application for its menu at that point |

Dragged nodes snap so that one of their edges lines up with the same edge of a nearby node - left with left, top with top - and so that a wire between a dragged node and a still one runs straight. Only nodes in view and close to the dragged ones are candidates. A faint guide between the two aligned nodes shows what snapped. Snapping to the grid is available when the application turns it on.

## Wiring

| Gesture | Effect |
| --- | --- |
| Drag from a pin to another pin | Connect them; while over the pin the preview shows the wire as it will be |
| Drag from a pin and release on empty canvas | Ask the application for a node to complete the wire |
| Drag from an input that is already wired | Start a new wire; landing it on a single-wire input replaces the old wire, dropping it on empty canvas asks for a node as above |
| Right-click a wire | Remove it |

A wire never connects a node to itself. Whether two pins are compatible beyond that is the application's decision.

## Cutting

| Gesture | Effect |
| --- | --- |
| Ctrl + drag (Cmd on macOS), starting on empty canvas or on a node | Draw a stroke; every wire and every node it crosses is marked as you go and removed when you release, chains closing over the removed nodes as with Delete |

## Inserting a node into a wire

| Gesture | Effect |
| --- | --- |
| Hover a wire, click its `+` | Ask the application for a node to splice into the wire at that point |
| Double-click a wire | The same, at the pointer |
| Drag a node onto a wire and release | Splice the node into the wire; the wire lights up while you hover it, and shows in the cut colour if the node has no pins to splice with |
| While dragging: hold Alt | Move past wires without splicing |

After an insert, the nodes downstream slide right and the nodes upstream slide left, as far as needed for the new node to overlap neither.
