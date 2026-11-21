# Portable libraries and selected-font history

Local library references are written relative to the saved document where possible. Moving or cloning a project preserves its links when the library and consumer files keep their relative locations. Save As rebases the serialized links while the open tab retains the origin of its existing history. Native versions and branches follow the same path mapping. Legacy absolute paths remain readable.

An opened document resolves relative links against its own location. Unsaved data with no known origin does not guess a working directory; its first explicit local Save As establishes that origin. Explicit library.add arguments still identify the file the user selected. Cache entries are resolved per document, so identical relative spellings in two projects remain independent. Automation read authority is checked before file access and cache reuse.

Stored version JSON remains lazy. Unrelated raw values retain their original representation. Duplicate object fields, malformed snapshots and non-object entries are preserved rather than silently repaired; such snapshots are outside the ordinary relocation guarantee.

Loading a selected font refreshes portable appearance for automatic and fixed-size text without discarding usable guest geometry when that named font is unavailable. Font-cache-only refreshes of imported library text remain distinct from authored changes. Saved-state history keeps real geometry and independent version-metadata edits dirty, and restores both history stacks and saved state when normalization panics or refuses a change.

The isolated library candidate passed25 controls,15 fresh-process CLI workflows and scoped macOS ARM64 native project-move, Save As, library-readability and reopen checks. The separate font/history candidate passed16 selected controls after the original produced11 failures and5 compatibility passes. One qualified external-font case is explicitly ignored by default and must also execute through the external-input qualification route. No test font is packaged in the app source or copied into its build-font input.

These are producer and scoped runtime results. Fresh combined CI, matching final artifacts and combined native/browser acceptance remain required. Bounded uniform Scale export is described in file-format-parity.md. PNG physical-density integration is a separate pending adaptation to the current profile-aware export API. Readiness estimates and the whole open-issue review remain unchanged.
