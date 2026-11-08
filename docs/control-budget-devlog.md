# Native control ingress admission (#1111)

The original per-frame bound allowed eight sockets to retain large raw buffers, parse expanded
JSON trees and clone request envelopes while waiting for the UI. A small eight-socket dense-JSON
regression fails against the original production implementation: every request reaches the queue.

This candidate adds a process-shared reservation ledger, fallible prepaid raw buffer growth and
allocation-free structural admission before serde parsing. Request fields move out of the parsed
envelope. Real UI payload/reply owners keep their reservations through timeout and delayed frames;
the authentication cancellation registry stores only state handles. No dependency or wire request
schema is added. The lower frame cap and conservative admission refusal are intentional protocol
limits documented in `control-protocol.md`.

The budget is an ingress allocation envelope, not allocator RSS, document size, engine journal,
command runtime or reply-size control. Validation receipts belong to this isolated source and
must not be attributed to an earlier delivered binary. The first candidate passed full CI, then a
second real-TCP regression exposed cancelled native entries retaining 49656 of a 65536-byte pool
without a UI tick. Native pruning now runs before admission and after the reply wait; cancellation
and deadline refusals retain their actual reason. Teardown explicitly stops native transport before
dropping input owners. The final source's validation is recorded in the accompanying handoff receipts.

A third bounded TCP regression held a terminal refusal socket open before FIN. The attached
backend sent its next distinct RPC into that closing connection and reported an ambiguous lost
reply. It now discards explicit null-ID refusals immediately and reconnects for the next call;
the refused operation is never replayed. Numeric-ID and missing-ID error controls retain their
connection. The old client failed this fixture while both controls passed; the fixed client
passes all nine backend checks.
