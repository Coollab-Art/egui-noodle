# Shortcuts and gestures

Every interaction the canvas offers, with its binding. Kept in step with the code: a gesture lands in the same commit as its line here.

## Navigating

| Gesture | Effect |
| --- | --- |
| Middle-drag or right-drag on the canvas | Pan |
| Mouse wheel | Zoom, around the pointer |
| Shift + wheel, Alt + wheel | Pan horizontally / vertically |
| Ctrl + wheel, pinch | Zoom, around the pointer |

The primary button is left free for selecting and dragging nodes.

## Selecting

| Gesture | Effect |
| --- | --- |
| Click a node | Select it alone |
| Ctrl + click a node (Cmd on macOS) | Add it to or remove it from the selection |
| Drag on empty canvas | Box-select every node the box touches |
| Shift + drag on empty canvas | Box-select, adding to the selection |
| Click empty canvas | Deselect all |

## Editing

| Gesture | Effect |
| --- | --- |
| Drag a node | Move it, and every other selected node with it |
| While dragging: hold Shift | Do not snap |
| Delete, Backspace | Delete the selected nodes |
| Right-click empty canvas | Ask the application for its menu at that point |

Dragged nodes snap to the edges and centres of the nodes around them, and to the grid when the application turns that on. An orange guide shows what they snapped to.

## Wiring

| Gesture | Effect |
| --- | --- |
| Drag from a pin to another pin | Connect them |
| Drag from a pin and release on empty canvas | Ask the application for a node to complete the wire |
| Drag from an input that is already wired | Pick the wire up: drop it on another input to move it, on empty canvas to remove it, anywhere else to leave it as it was |
| Right-click a wire | Remove it |

A wire never connects a node to itself. Whether two pins are compatible beyond that is the application's decision.

## Cutting wires

| Gesture | Effect |
| --- | --- |
| Ctrl + drag (Cmd on macOS), starting on empty canvas or on a node | Draw a stroke; every wire it crosses is marked as you go and removed when you release |

## Inserting a node into a wire

| Gesture | Effect |
| --- | --- |
| Hover a wire, click its `+` | Ask the application for a node to splice into the wire at that point |
| Drag a node onto a wire and release | Splice the node into the wire; the wire lights up while you hover it, and shows in the cut colour if the node has no pins to splice with |
| While dragging: hold Alt | Move past wires without splicing |

After an insert, the nodes downstream slide right to make room for the new one.
