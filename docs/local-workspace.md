# Local workspace features

This fork keeps local documents, plugins, preferences and recovery separate from online sharing. Native macOS, the CLI and the hosted browser have different storage capabilities.

## Native data and session restore

Start with `figcraft --data-dir /absolute/path/to/workspace-data` or set `FIGCRAFT_LOCAL_DATA_DIR`. The directory is selected before fonts, preferences, recovery, projects, plugins or control credentials are initialized. It contains `state.json`, `projects.json`, recovery, plugin, and downloaded font data. Existing installed system fonts are read, not copied or modified. An explicit relative path or a final symlink is refused.

Normal launches can reopen local saved tabs and windows, including their page, view, panel and logical window geometry. Unsaved documents remain the recovery system's responsibility. Online tabs are not restored in this fork. `--fresh` skips automatic restore; Restore last session remains available. A failed restore starts at Home and offers manual recovery instead of retrying forever.

`--control PORT` starts without automatic session restore or session persistence so automation does not replace the user's saved window layout. Add `--persist-session` explicitly when testing or using that behavior. `--data-dir` is recommended for every automated GUI launch.

## Authenticated local control

The optional loopback control listener is off by default. Enable it explicitly with `--control PORT` or through Local MCP server settings. Each listener lease generates a fresh private credential file, `control-PORT.token`, inside its data directory. It is retired on disable, rebind or orderly exit, without deleting a newer listener's replacement file. Failure to bind or write credentials leaves control disabled. A failed rebind preserves the previous working listener.

The first JSON line of every TCP connection must be `{"id":0,"method":"auth","token":"<contents of private token file>"}`. It has a 1 KiB limit and a total two-second deadline. Wait for the successful protocol-1 acknowledgement before sending ordinary control requests. Never log or place the token contents in command arguments. Same-user processes that can read the private file remain within the user's authority. Already admitted/running work is not rolled back when the listener is disabled.

Attach the CLI with `figcraft-cli mcp --attach PORT --control-token-file /absolute/path/to/control-PORT.token`. `FIGCRAFT_CONTROL_TOKEN_FILE` selects a file too; with `FIGCRAFT_LOCAL_DATA_DIR`, the CLI derives its port-specific file. Missing or malformed credentials fail closed. Authentication repeats on reconnect; uncertain mutations are never replayed. Copy configuration in the MCP dialog includes the file path, not its secret. This fork never edits global MCP client configurations; `--snippet CLIENT` only prints reviewable configuration (`--install` is a compatibility alias with the same read-only behavior).

## Revisions and document events

`document.inspect` reports a process-local revision plus `conditionalExecute` and `documentEvents` capabilities. A guarded `engine.execute` may carry `expected: {document, revision}`. Both numbers must be nonnegative safe integers, and the document UID must match the actual target. Guard validation and comparison happen on the UI/engine thread before command effects. A stale or malformed guard applies nothing; reconnecting requires a new inspection.

`events.poll {after?, topics?:["document"], limit?}` emits bounded `document.changed` invalidations for committed changes from commands, direct UI edits and completed gestures. Preview-only and failed/rolled-back changes emit none. These are invalidations, not a durable change journal: inspect again when `reset` is true or after reconnect. The log holds at most 1,024 events, changed-node IDs cap at 256, individual data caps at 8 KiB, and text fields are bounded. Browser polling is nonblocking; native MCP may long-poll for at most 25 seconds. The hosted CLI exposes `events`; web MCP exposes `web_events` only when the browser advertises support.

## Projects and plugins

Project folders and favorites organize references. They do not move or delete document bytes. The native library persists beside preferences and imports disk folders without following links. Its limits are 5,000 folders, 50,000 files and 32 levels. Damaged metadata is preserved and recovered from backup; an unreadable store is never silently overwritten.

The hosted browser has its own bounded logical project store, folder selector, favorites, drag-and-drop moves and folder actions. Its limits are 2,048 folders, 16,384 files and 32 levels. Removing a browser folder reparents its children and files. Workspace backups include valid project metadata; damaged metadata is retained only as inert quarantine data. Revision conflicts prevent stale writes through a running store instance; this is not a cross-process database transaction guarantee.

The built-in plugin store embeds real compiled Counter, Export names, Swatch grid and Tidy names modules. It never reads replacements from the working directory. Native development supports create, build, reload and a bounded run console. Generated projects use the current SDK API and preserve existing source files. Installed runtimes may supply the SDK using `FIGCRAFT_PLUGIN_SDK`; Rust and the WebAssembly target are development prerequisites. Browser users can run built-ins and inspect the console; native filesystem scaffolding and compilation use the CLI.

## External files, review and updates

Clean native tabs refresh after another process saves. Dirty edits, active gestures/text previews, live documents and branch tabs are preserved. Explicit save paths, aliases and queued saves retain disk-conflict checks. A failed replacement keeps the last good document and backs off before retrying. `file.refresh` is refused inside batches/groups because saved disk baselines cannot be undone transactionally.

Review design audits selections, pages or documents against 26 layout/accessibility rules. It offers scoped fixes, ignore, comments and assertion thresholds through the same engine commands used by CLI/MCP. Report-only findings explain why a safe automatic fix is unavailable.

The native macOS menu uses the same catalog and focused-window command routing as the in-app menu. The bundle advertises .fig/.jam Open With support as an alternate viewer; it does not select itself as the default.

Update checks report official upstream releases. This local build has additional repairs, so automatic official-package download, installation and replacement are disabled. Replace the kit manually with a verified local build. `FIGCRAFT_NO_UPDATE_CHECK=1` skips startup checks.
