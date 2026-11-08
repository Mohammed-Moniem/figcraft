# Attached MCP relay

The optional figcraft-attach binary serves the same MCP tools, resources and prompts through a running native app, without an embedded document engine, renderer, font store or headless session. The full figcraft-cli remains available for headless commands.

Build locally with cargo build --locked --release -p figcraft-attach. Configure a client by changing its executable path to this binary while keeping args ["mcp", "--attach", "PORT", "--control-token-file", "/absolute/private/control-PORT.token"]. Token contents must never appear in arguments or configuration; the file path is sufficient. The relay also accepts FIGCRAFT_CONTROL_TOKEN_FILE, or derives the file from FIGCRAFT_LOCAL_DATA_DIR and the port. Missing credentials fail closed with usage exit2; unreadable/refused credentials or an unavailable app fail with exit1.

Attach is required. Local file access is enforced by the running app’s automation roots; the relay refuses local root flags rather than claiming to confine the app. There is no fallback session, MCP configuration writer, font scan or release installer in this package. --timeout SECONDS accepts1–600, default60. Errors and startup diagnostics use stderr; stdout is MCP JSON only.

Per-tool document selectors, window screenshots, client attribution, framing bounds and no-replay behavior are shared with the full CLI. A lost reply may follow an applied edit: inspect state before retrying. An explicit subsequent call can reconnect and authenticate. Protocol-1 native authentication is mandatory; older unauthenticated app listeners are not a supported fallback.

The default headless feature on figcraft-mcp keeps its existing Headless/automation-root APIs. This binary disables it; shared JSON coercion/inspection helpers live below the engine so neither validation nor the tool catalogue is duplicated. Build the relay separately from the full CLI when measuring size, since a joint workspace build unifies dependency features.

This local fork’s dependency reuse approval is restricted to implementation/testing. No official download, distribution, signing/notarization, browser or cross-platform acceptance is implied. Existing SPDX expressions and global license policy remain unchanged. Use the source-bound validation receipts for measured artifact sizes and platform scope.
