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
