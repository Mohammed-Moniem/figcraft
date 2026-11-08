# Unlock a layer on the canvas

With Move or Scale selected, hover a locked layer to show its lock button near the upper-left
corner. Click the button to unlock that layer. Right-click the locked layer for **Unlock <name>**.
Both controls name the actual owner of the lock; a layer locked by its parent unlocks that parent.
If both a parent and child have explicit locks, unlock the parent first, then the child.

Unlocking keeps an unrelated selection and is one Undo step. Locked layers still cannot be
dragged. Hidden, clipped or masked-out content does not expose a lock control, and an unlocked
layer in front takes precedence. Switching files/pages or moving a file to another window retires
the old control and menu. Read-only version previews and view-only sessions remain protected.

Scripts can use the same existing command with explicit targets:

```json
{"command":"object.toggleLock","params":{"ids":[42],"locked":false}}
```

The button uses FigCraft's existing original lock icon; no external artwork is included.
