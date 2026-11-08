# Connection modes and file boundaries

## Headless full CLI

`figcraft-cli mcp` serves a fresh in-process session over stdio. Keep one server process for a
workflow; starting a second process creates a different document. Use the host's MCP setup if
already configured. This skill does not configure it. The CLI's `mcp --snippet CLIENT` prints a
reviewable snippet; `mcp --install` writes client settings and is a separate explicit action.

For a confined session, pre-create the output directory and start:

```sh
figcraft-cli --no-system-fonts mcp --automation-write-root /absolute/work/design
```

Write roots also permit reads. Add repeatable `--automation-read-root` flags for input folders.
Without roots the CLI runs with the current user's file access. A read root alone denies writes.
Use `--no-system-fonts` only when built-in fonts are the intended input; otherwise inspect the
font report and rendered text rather than silently accepting substitutes.

## Attached desktop

The user must have an app running with a control port, for example
`figcraft --control 47800`, confined by that app's own automation roots if needed. Start a client
with its actual owner-only token file:

```sh
figcraft-cli mcp --attach 47800 --control-token-file /absolute/private/control-47800.token
```

An available `figcraft-attach --attach ...` is a smaller attach-only alternative; probe its own
`--help`, since it does not provide headless commands. Never print the token's contents, put
them in JSON, or loosen file permissions. A bare port means loopback. Attached mode refuses
client automation-root flags: the running app supplies the policy. It requires compatible auth
and has no automatic headless fallback.

Inspect the current window and document before editing. Explicit window/document targets should
come from live inspection, not an earlier tab order. If the owner closes or changes, inspect
again and stop dependent edits when the intended document cannot be established. UI screenshot
and pointer tools need the actual desktop backend; headless page rendering is not UI acceptance.

If an attached request times out, first inspect whether it already took effect. A request that
started can complete after the wait expired, so never replay a mutation blindly. Keep the app
responsive/visible and use a small read-only request to diagnose it. Current clients support
`--timeout SECONDS` (1–600) for attached requests; this changes waiting, not completion or replay
semantics. Do not promise that a timeout or covered-window problem is fixed merely by raising it.

## Native save versus export

MCP `save_file {"path":"/absolute/work/design/card.figcraft"}` saves the native document.
Use `export` with the current tool schema (`format`, `path`, optional `ids`, `scale`) for an
image or vector output. Inspect its warnings. A `.fig` save/export is an experimental interchange
copy and can omit data; keep `.figcraft` as the editable source. For CLI flags, read
`figcraft-cli export --help` and `docs/cli.md` in the source checkout.

Do not send programmatic dialog-opening commands and assume a file was saved. Supply explicit
paths; the headless/control paths drop dialog requests.
