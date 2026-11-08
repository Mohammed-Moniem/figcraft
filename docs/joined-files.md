# Joined files on Home

Home ▸ Online remembers up to 100 files successfully joined with a link or code, most recently
opened first. Each row shows the file's name, server, negotiated access and last opened date.
The document must arrive before a join is remembered; failed or cancelled joins add no history.

Open uses the saved invite capability. View and comment links retain their access; an open tab
using that same capability is focused. Rejoining successfully replaces an older capability for
the same complete server endpoint and file. Repeated Open requests for that exact capability
reuse its pending join too; the last-opened time waits for document synchronization.
Protocol, port and case-sensitive proxy paths remain
separate. The default publishing server is unchanged.

The row menu's Forget on this computer (in the browser, Forget on this browser) removes local
history. It does not revoke the server's link or close the document. A welcome already on its way,
including one arriving after a failure or reconnect, cannot recreate the forgotten entry. A new
explicit join can remember the file again. Each existing session retains at most 100 suppression
identities; after that it continues joining normally but no longer records history.

Owned files keep their existing owner record. Home hides a duplicate joined card for the same
endpoint/file while retaining its private capability: the browser needs it after reload because
it does not persist owner credentials. Files published with a server key can be remembered after
their successful document join without becoming locally owned files.

## Local persistence

Native preferences include the private invite field with the existing atomic preferences writer
and owner-only Unix permissions. A failed native write preserves the previous version and shows
a fixed, payload-free message through the app's error UI. The browser keeps a bounded version-1 `figcraft.kept.v1` item in
the page origin's storage, containing only the name and joined records; it migrates the older
`figcraft.userName` name. It does not serialize the complete preferences or add owner/server keys.
Restoring history never connects to a server; opening a row does. Hidden-tab logic also saves
completed joins and Forget without waiting for a painted frame.

If browser storage read operations fail, writes stay blocked for that visit and a fixed message explains the
failure. Reload after storage is available again. A failed write leaves in-memory operations
usable, but the old durable entry can return after reload; a fixed message says changes may be
lost. Exceptions and storage payloads are not logged or shown. Invalid records are skipped without
discarding unrelated preferences. Browser input is capped at 512 KiB; links retain the existing
2048-byte invite-parser limit.
Successfully read malformed or unsupported versions fall back to the legacy name and may be
replaced by a later save; this differs from an I/O failure that leaves unreadable history untouched.

Server base paths retain the existing 256-byte budget. When parsing adds a missing trailing slash,
the canonical form can reach 257 bytes; only that single trailing slash is accepted beyond the
base budget, so saved and published links round-trip. Longer paths, a 257-byte unsuffixed path,
and a 257-byte path ending in two slashes remain refused. Host, route-character, query and token
validation are otherwise unchanged.

Invite capabilities are private preference fields. The new record's Debug output and the three
history command responses omit them, and they are not added to saved documents. Existing
collaboration commands retain their current authorized share-link behavior.

## Commands and scope

- `collab.joinedFiles {}` lists local `{address, file, name, access, opened}` metadata without
  contacting servers.
- `collab.openJoined {"file":"f","address":"wss://example.invalid/Shared/"}` opens a saved
  capability; `address` is required when multiple endpoints have that file id.
- `collab.forgetJoined` takes the same selector and removes local history only.

This local implementation addresses the persistence/reopen/forget behavior in
[#1881](https://github.com/storytold/figcraft/issues/1881) and
[#1882](https://github.com/storytold/figcraft/issues/1882). The related
[PR1999](https://github.com/storytold/figcraft/pull/1999), inspected at
`d4b772743ac449b70a2f74c6616c04ac1aa71c07`, was open and unmerged when reviewed; it covers desktop
history and leaves browser persistence unresolved. Its optional preview/reachability subsystem
is outside this change. No new health probes, account setup or parent host/provider lifecycle
changes are introduced. Independent native/browser acceptance and combined CI remain separate
from scoped regression evidence; product readiness estimates are unchanged.
