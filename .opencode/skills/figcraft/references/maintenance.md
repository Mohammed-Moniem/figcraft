# Local capability checks and scope

OpenCode discovers `.opencode/skills/figcraft/SKILL.md` in a project; its ID is the directory
name, `figcraft`. Current OpenCode also accepts global skill directories, but this repository
does not install anything there. For another project copy this entire `figcraft` directory into
that project's `.opencode/skills/`, preserving an existing directory for review. Start OpenCode
in that project and use `@figcraft`; the current skill tool loads `{"id":"figcraft"}`. Read the
[official skills documentation](https://opencode.ai/docs/skills/) for your installed host version.
This skill does not require an OpenCode configuration file or an MCP configuration change.

Optional scripts use Python3.9+ standard library only. Run them with `python3`; no
package manager is needed. `--cli` is a single executable path, including paths with spaces,
not a shell command. Probes have bounded time/output, reap their own CLI child and never invoke
tools or attach. The temporary headless probe permits reads only inside an empty temporary
directory and denies writes. It lists advertised tools; availability of desktop-only tools must
still be checked against the chosen backend.

For a release, run against the exact release binary and retain its identity beside the output:

```sh
python3 scripts/catalog.py probe --cli /absolute/release/figcraft-cli > /absolute/review/probe.json
python3 scripts/catalog.py commands --cli /absolute/release/figcraft-cli > /absolute/review/commands.json
```

These commands print JSON and never update this skill automatically. Version/counts and command
descriptions come from that binary. Keep them in release verification evidence, not a stale
hardcoded catalog shipped here. A catalog/probe error is a failure; do not interpret partial
output as a compatible API.

`python3 scripts/catalog.py release` makes one optional unauthenticated HTTPS request to the
official repository's latest-release metadata endpoint. It does not follow redirects, retry,
download assets, compare arbitrary version strings or install anything. Offline, rate-limit,
invalid and oversized responses fail clearly. Local capability discovery works without it.

The native CLI and MCP workflow can be tested without an OpenCode provider. Host discovery,
skill loading, live model tool selection, attached desktop behavior and cross-platform script
execution require their own acceptance; passing script tests does not establish those outcomes.
Upstream [CLI](https://github.com/storytold/figcraft/blob/main/docs/cli.md),
[MCP](https://github.com/storytold/figcraft/blob/main/docs/mcp.md) and
[control](https://github.com/storytold/figcraft/blob/main/docs/control-protocol.md) documentation
explain the application contracts. The installed binary's help/schema remain the capability source.
