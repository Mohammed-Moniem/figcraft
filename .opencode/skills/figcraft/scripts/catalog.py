#!/usr/bin/env python3
"""Read installed FigCraft capabilities; never install, attach, or change settings."""

import argparse
import json
import re
import subprocess
import sys
import tempfile
import threading
import time
import urllib.error
import urllib.request

MAX_OUTPUT = 4 * 1024 * 1024
MAX_RELEASE = 256 * 1024
RELEASE_URL = "https://api.github.com/repos/storytold/figcraft/releases/latest"
NAME = re.compile(r"[A-Za-z0-9_.-]{1,128}\Z")


class ProbeError(Exception):
    """A bounded probe failed; no capability result should be trusted."""


def run_cli(cli, args, timeout, input_data=b""):
    """Read both pipes with a shared cap and reap only the child this call starts."""
    try:
        child = subprocess.Popen(
            [cli, *args], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
    except OSError as error:
        raise ProbeError("cannot start figcraft-cli; pass --cli with its local executable") from error
    output = [bytearray(), bytearray()]
    lock = threading.Lock()
    failures = []

    def read_pipe(pipe, index):
        try:
            while True:
                chunk = pipe.read(8192)
                if not chunk:
                    break
                with lock:
                    if sum(map(len, output)) + len(chunk) > MAX_OUTPUT:
                        failures.append("CLI output exceeded the 4 MiB probe limit")
                        child.kill()
                        break
                    output[index].extend(chunk)
        except OSError:
            failures.append("could not read CLI output")
        finally:
            pipe.close()

    readers = [threading.Thread(target=read_pipe, args=(pipe, index), daemon=True)
               for index, pipe in enumerate((child.stdout, child.stderr))]
    for reader in readers:
        reader.start()
    try:
        child.stdin.write(input_data)
        child.stdin.close()
        child.wait(timeout=timeout)
    except subprocess.TimeoutExpired as error:
        child.kill()
        child.wait()
        raise ProbeError("CLI probe timed out; its child was stopped") from error
    except (BrokenPipeError, OSError) as error:
        child.kill()
        child.wait()
        raise ProbeError("CLI closed its probe input unexpectedly") from error
    finally:
        child.stdin.close()
        for reader in readers:
            reader.join(timeout=1)
    if any(reader.is_alive() for reader in readers):
        raise ProbeError("CLI output pipes did not close after the child exited")
    if failures:
        raise ProbeError(failures[0])
    if child.returncode:
        # Do not echo stderr: an arbitrary executable or environment may include secrets.
        raise ProbeError(f"CLI probe failed (exit {child.returncode}); check its --help directly")
    try:
        return bytes(output[0]).decode("utf-8")
    except UnicodeDecodeError as error:
        raise ProbeError("CLI output was not UTF-8") from error


def read_json(text):
    def invalid_constant(_):
        raise ValueError("non-finite JSON value")
    try:
        return json.loads(text, parse_constant=invalid_constant)
    except (ValueError, RecursionError) as error:
        raise ProbeError("probe returned malformed JSON") from error


def commands(cli, timeout, filter_text=None):
    args = ["commands", "--json"]
    if filter_text is not None:
        args.append("--filter=" + filter_text)
    value = read_json(run_cli(cli, args, timeout))
    if not isinstance(value, list) or len(value) > 10000:
        raise ProbeError("command catalog is not a bounded JSON array")
    ids = []
    for command in value:
        if (not isinstance(command, dict) or not isinstance(command.get("id"), str)
                or not NAME.fullmatch(command["id"]) or not isinstance(command.get("params"), str)):
            raise ProbeError("command catalog has an invalid command entry")
        ids.append(command["id"])
    if len(set(ids)) != len(ids):
        raise ProbeError("command catalog contains duplicate ids")
    return value


def probe(cli, timeout):
    version = run_cli(cli, ["--version"], timeout).strip()
    if not version.startswith("figcraft-cli ") or len(version) > 256 or any(ord(c) < 32 for c in version):
        raise ProbeError("unexpected figcraft-cli version response")
    catalog = commands(cli, timeout)
    requests = [
        {"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
            "protocolVersion": "2025-06-18", "capabilities": {},
            "clientInfo": {"name": "FigCraftSkillProbe", "version": "1"}}},
        {"jsonrpc": "2.0", "method": "notifications/initialized", "params": {}},
        {"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}},
    ]
    payload = "".join(json.dumps(request) + "\n" for request in requests).encode()
    # A fresh empty read root denies all file writes. No attach, install, or tool call occurs.
    with tempfile.TemporaryDirectory(prefix="figcraft-skill-probe-") as directory:
        text = run_cli(cli, ["--no-system-fonts", "mcp", "--automation-read-root", directory], timeout, payload)
    replies = {}
    for line in text.splitlines():
        reply = read_json(line)
        if not isinstance(reply, dict) or reply.get("jsonrpc") != "2.0":
            raise ProbeError("invalid MCP response envelope")
        request_id = reply.get("id")
        if type(request_id) is not int or request_id not in (1, 2) or request_id in replies or "error" in reply:
            raise ProbeError("MCP probe returned an error, duplicate, or unexpected response")
        replies[request_id] = reply.get("result")
    initialized, listed = replies.get(1), replies.get(2)
    if not isinstance(initialized, dict) or not isinstance(listed, dict):
        raise ProbeError("MCP initialize or tools/list response is missing")
    if initialized.get("protocolVersion") not in ("2025-06-18", "2025-03-26", "2024-11-05"):
        raise ProbeError("MCP negotiated an unsupported protocol revision")
    server = initialized.get("serverInfo")
    if not isinstance(server, dict) or any(not isinstance(server.get(key), str) for key in ("name", "version")):
        raise ProbeError("MCP server identity is missing")
    tools = listed.get("tools")
    if not isinstance(tools, list) or not 1 <= len(tools) <= 512:
        raise ProbeError("invalid MCP tool catalog")
    names = [tool.get("name") if isinstance(tool, dict) else None for tool in tools]
    if any(not isinstance(name, str) or not NAME.fullmatch(name) for name in names) or len(set(names)) != len(names):
        raise ProbeError("invalid or duplicate MCP tool names")
    return {"cliVersion": version, "commandCount": len(catalog), "toolCount": len(names),
            "tools": names, "protocolVersion": initialized.get("protocolVersion"),
            "serverInfo": initialized.get("serverInfo"), "mode": "fresh read-confined headless session"}


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        return None


def latest_release(timeout, opener=None):
    """Explicit, single public metadata GET. No credentials, redirects, retries, or assets."""
    if opener is None:
        opener = urllib.request.build_opener(NoRedirect()).open
    request = urllib.request.Request(RELEASE_URL, headers={
        "Accept": "application/vnd.github+json", "User-Agent": "FigCraft-skill-probe"})
    deadline = time.monotonic() + timeout
    try:
        with opener(request, timeout=timeout) as response:
            data = bytearray()
            while len(data) <= MAX_RELEASE:
                if time.monotonic() >= deadline:
                    raise ProbeError("release metadata timed out; no update action taken")
                chunk = response.read(min(1024, MAX_RELEASE + 1 - len(data)))
                if not chunk:
                    break
                data.extend(chunk)
    except urllib.error.HTTPError as error:
        raise ProbeError(f"release metadata refused (HTTP {error.code}); stop, do not retry automatically") from error
    except (OSError, urllib.error.URLError) as error:
        raise ProbeError("release metadata unavailable; check network access or continue with the installed catalog") from error
    if len(data) > MAX_RELEASE:
        raise ProbeError("release metadata exceeded the 256 KiB limit")
    try:
        value = read_json(data.decode("utf-8"))
    except UnicodeDecodeError as error:
        raise ProbeError("release metadata was not UTF-8") from error
    tag = value.get("tag_name") if isinstance(value, dict) else None
    url = value.get("html_url") if isinstance(value, dict) else None
    if (not isinstance(tag, str) or not 1 <= len(tag) <= 128 or any(ord(c) < 32 for c in tag)
            or not isinstance(url, str) or url != "https://github.com/storytold/figcraft/releases/tag/" + tag):
        raise ProbeError("unexpected release metadata; no update action taken")
    return {"tag": tag, "url": url, "action": "metadata only; nothing downloaded or installed"}


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="action", required=True)
    for action in ("probe", "commands", "release"):
        command = sub.add_parser(action)
        command.add_argument("--timeout", type=float, default=10)
        if action != "release":
            command.add_argument("--cli", default="figcraft-cli", help="one local executable, never a shell command")
        if action == "commands":
            command.add_argument("--filter")
    args = parser.parse_args(argv)
    if not 0 < args.timeout <= 60:
        parser.error("--timeout must be greater than 0 and at most 60 seconds")
    try:
        if args.action == "probe":
            value = probe(args.cli, args.timeout)
        elif args.action == "commands":
            value = commands(args.cli, args.timeout, args.filter)
        else:
            value = latest_release(args.timeout)
        print(json.dumps(value, ensure_ascii=False, indent=2, allow_nan=False))
        return 0
    except ProbeError as error:
        print("figcraft skill: " + str(error), file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
