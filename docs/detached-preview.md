# Separate prototype previews

Open an inline prototype preview, then choose **Pop out preview** in its header. On native hosts the same player moves into a decorated, resizable window. It keeps its current screen, navigation history, overlays and runtime variable changes. Held keys, pointer presses and controller buttons are released at transfer. The companion follows edits to its source file while another editor tab is active; interactions continue on the player’s private copy.

A companion does not own or move editor tabs. Moving the source tab into another editor window keeps its preview attached to the same live document. Closing the companion resets only that player. Closing, reverting or reopening the source with a different live document identity retires the companion. Reopening Preview starts an inline player. At most eight companions can be open, with one per source document; another Pop out request focuses the existing companion.

Embedded hosts, including the browser, keep the inline preview. A screen reader also keeps it inline because the pinned native integration supplies an accessibility adapter only for the main viewport. The header action remains discoverable and explains why it is disabled.

The public non-undoable command is `proto.detachPreview {start?}`. The same validated top-level-frame rules as `proto.present` apply. The chosen frame starts a fresh player only when no inline player exists; moving an existing player preserves its state. The UI request captures the source document UID when the command runs. Headless commands can request this UI but cannot create a native host.

The control channel lists companions in `window.list` with `kind: "preview"`, a numeric `sourceDocument`, and a non-reused lifetime `generation`. Editor windows use `kind: "editor"`. `window.focus`, `ui.inspect`, `ui.input` and `ui.screenshot` support preview windows. Use `ui.input` for actual prototype pointer/key/controller input. Design methods such as `engine.execute`, `ui.pointer`, `ui.key` and `ui.type` refuse preview targets; explicitly target an editor window for them. Existing authentication, document conditions, input admission, cancellation and screenshot budgets apply unchanged.

Each companion uses its own viewport bounds and pixel scale. Local tests simulate separate viewports, focus, positions and DPI. Physical multi-monitor movement, OS-specific window handling and matching combined native/browser acceptance require separate runtime evidence; those are not established by a headless viewport test.

Source edits wake only their detached preview, including after background control/live work when an editor does not paint. Once the child draws the current source, unchanged source identity does not schedule more child frames.
