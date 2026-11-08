# Cross-axis Fill in a hugging container

A child of a horizontal auto-layout row can fill its height while the row hugs its height. A child
of a vertical column can fill its width while the column hugs its width. Changing that child to
Fill keeps the parent's Hug setting. The child's resolved cross size follows the content extent
of its line, including the parent's padding and applicable child bounds.

Existing component instances update in the same command as their main. One Undo restores the
previous document; native save and reopen retain the sizing choices. Main-axis Fill keeps the
existing rule that makes a hugging parent fixed. Grid sizing and absolutely positioned children
retain their own rules.

The layout pass includes a cross-axis Fill child's minimum contribution when measuring a hugging
line. All-Fill lines use their resolved-size fallback. For a nonwrapped line with an already known
cross size, stretching a hugging child happens before measuring the parent's main extent; this
keeps a nested text card's reflow and its surrounding container consistent.

Validation on the isolated producer recorded the original public command failures and layout
controls, then 2,224 passing engine/layout tests, three existing ignored tests, strict Clippy,
formatting and layer checks. Matching combined native/browser acceptance is pending. The
wrapped-line controls use rectangular children; arbitrary nested wrapped-text dependency order,
new intrinsic measurement rules and full CSS or incumbent layout parity are unverified.
