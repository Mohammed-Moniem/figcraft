# Selected browser preferences

The browser keeps these settings for the same page origin across reloads: selection-outline
opacity, pixel-grid size, UI accent, platform keyboard shortcut overrides, focused number-wheel
editing and the optional repeated-menu setting. Their defaults and native preferences are unchanged. Document content, dirty state and
Undo are independent of these settings.

The explicit version-1 `figcraft.preferences.v1` item contains only those six fields and its
version. It is capped at 1 MiB before parsing or writing. It never contains server/owner keys,
joined capabilities, native paths, documents, recovery state, plugins or account/provider data.
The separate `figcraft.kept.v1` name/joined-file record retains its existing bounded schema and
capability policy; see [Joined files](joined-files.md). No full preferences object is serialized.

Each field uses its established validation/defaults. Fractional grid/opacity values stay exact;
accents normalize to opaque six-digit sRGB. Shortcuts use the existing typed reader: at most
512 commands per platform, eight chords per command and 80 bytes per chord, with known command,
reserved-key and conflict checks. Bad individual fields fall back without discarding valid ones.
An absent item keeps the visit's defaults and preserves legacy name migration.

The optional Boolean `keep_menu_open` defaults to false for legacy records or malformed field
values. See [repeated menu options](menu-options.md). It adds no document or authority data to
the appearance record and retains the same version and recovery policy.

A failed read or malformed/unsupported top-level appearance record blocks writes to that record
for the visit, preserving its raw bytes. Reload after storage is usable or its schema supported.
This is stricter than the older joined record's documented malformed-schema fallback. Write
failures leave settings usable in memory and report that changes may be lost after reload.
Appearance and joined/name records have independent write flags; one failure does not prevent
an attempt to keep the other. Each replacement is one atomic storage-item write. Browser error
payloads, exceptions and private values are never included in the generic UI feedback.

In host mode, a nonblank `?author=` retains its existing session attribution behavior. An
appearance-only save keeps the pre-author local name rather than retaining the injected name.
The name control remains editable: once a different name is committed and observed by a save,
it is kept normally, including a later explicit edit back to the original host text. An exact
same-name commit without an observed change cannot be distinguished from another preference
save; it does not infer consent to retain a visit-only author. Blank-author and normal visits
keep their usual immediate name saves. The host's server, authority, roots and lifecycle do not
come from this appearance record.

The original browser reload failure is separate from scoped codec tests. Acceptance requires a
fresh exact-source browser build, same-origin reload and viewed UI screenshots; native/in-process
checks alone do not establish browser, cross-platform or whole-product completion.
