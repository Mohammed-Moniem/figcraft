# Library links transferred between documents

A native file import resolves newly adopted relative local library links against the input
file's explicit path. A foreign layer paste resolves those links against the copied snapshot's
retained library origin. The in-memory copies use absolute local paths; the existing native
writer can make them relative to the next save destination.

Save As does not change the meaning of older document snapshots. The clipboard therefore keeps
the library origin separately from `source_path`, whose published-source behavior is unchanged.
Branch copies use the existing main-file origin lookup. A destination link already enabled under
the same library ID remains authoritative, including its cached assets and path.

A file supplied only as bytes, or a foreign clipboard without an origin, cannot introduce a new
relative local reference: the transfer fails with an origin error and the command rolls back.
A filename hint, the destination folder and the working directory supply no replacement origin.
An explicit absolute local link needs no origin. A same-document layer paste keeps its existing
cache without interpreting the links again.

Blocked network and device references remain verbatim. This conversion performs no file or
network access and grants no new authority; library access still passes through the existing
local/network and automation read-root checks, including when assets were already cached.
Imported library assets keep their existing read-only rules.
