# Native instance scaling

Status: **IN_PROGRESS**. This local extension addresses the native variable and shared-style
scaling portion of [#2090](https://github.com/storytold/figcraft/issues/2090). The reviewed
implementation is locally merged; combined CI and matching native/browser acceptance are pending.

The K tool stores a relative scale context on native component instances. Bound lengths and linked
style dimensions use the product of their ancestor contexts, while the global variable and style
definitions keep their original units. Nested instances, later main-component edits, reset and
native save/open retain that context. Ordinary resizing keeps its existing behavior.

Lengths, absolute text dimensions and tiled-image scale follow physical scaling. Percentages,
angles, colors, counts, relative text units and normalized coordinates retain their meanings.
Style creation or capture from a scaled layer checks that its global definition reproduces the
visible result before mutation. A compatible existing definition can be reused, including a
clamped value. A changed capture that cannot reproduce the visible result is refused atomically.
This does not recover a unique value from an earlier clamped result.

Derived dimensional overrides continue accepting later main-component bindings. Explicit local
field edits, style replacement and detach keep local ownership. Global style update, bind and
delete propagate without becoming local edits; Repeat and mixed command batches retain that
distinction. Selected text ranges use their layer's scale context.

The isolated engine, document, collaboration and format suites pass 3,272 tests with seven existing
ignored tests, including 54 native-scale controls. Strict all-target Clippy, owned formatting and
layer checks pass on that producer. One exact cross-axis Fill/Hug test is excluded there because
its immutable baseline predates the separate [Fill/Hug repair](cross-axis-fill.md); it must run on
the combined source. The guarded merge also preserves [manual dash phase](dash-phase.md) scaling
and the existing Board and Fill/Hug registrations.

Actual K-handle gestures, combined shared-style and text-wrap behavior, launched native/browser
output and foreign import composition remain acceptance work. This is not full Figma, foreign
format, remote-library or cross-platform parity, and does not complete all of #2090.
