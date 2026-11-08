"""Local protocol/failure controls; no network, global settings, or application launch."""

import contextlib
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch
import urllib.error

import catalog


class CatalogTests(unittest.TestCase):
    def test_probe_uses_fresh_read_confined_session_and_current_catalog(self):
        calls = []

        def run(cli, args, timeout, input_data=b""):
            calls.append((cli, args))
            self.assertEqual(cli, "/path with spaces/full-cli")
            if args == ["--version"]:
                return "figcraft-cli 42.0.1\n"
            if args == ["commands", "--json"]:
                return json.dumps([{"id": "node.create", "params": "{type}"}])
            self.assertEqual(args[:3], ["--no-system-fonts", "mcp", "--automation-read-root"])
            self.assertTrue(Path(args[3]).is_dir())
            self.assertEqual(list(Path(args[3]).iterdir()), [])
            requests = [json.loads(line) for line in input_data.splitlines()]
            self.assertEqual([r["method"] for r in requests], ["initialize", "notifications/initialized", "tools/list"])
            return self.replies()

        with patch.object(catalog, "run_cli", side_effect=run):
            result = catalog.probe("/path with spaces/full-cli", 2)
        self.assertEqual(result["cliVersion"], "figcraft-cli 42.0.1")
        self.assertEqual((result["commandCount"], result["toolCount"]), (1, 1))
        self.assertEqual(result["tools"], ["get_node"])
        self.assertFalse(Path(calls[-1][1][3]).exists(), "temporary root must be removed")

    @staticmethod
    def replies():
        return "\n".join(json.dumps(reply) for reply in [
            {"jsonrpc": "2.0", "id": 1, "result": {"protocolVersion": "2025-06-18", "serverInfo": {"name": "figcraft", "version": "42"}}},
            {"jsonrpc": "2.0", "id": 2, "result": {"tools": [{"name": "get_node"}]}},
        ])

    def test_catalog_rejects_malformed_duplicate_and_nonfinite_json(self):
        for value in ["not-json", "NaN", "{}", '[{"id":"a","params":""},{"id":"a","params":""}]', '[{"id":"bad id","params":""}]']:
            with self.subTest(value=value), patch.object(catalog, "run_cli", return_value=value):
                with self.assertRaises(catalog.ProbeError):
                    catalog.commands("cli", 1)

    def test_filter_is_one_literal_argument(self):
        with patch.object(catalog, "run_cli", return_value="[]") as run:
            self.assertEqual(catalog.commands("cli", 1, "--help; no-shell"), [])
            self.assertEqual(run.call_args.args[1], ["commands", "--json", "--filter=--help; no-shell"])

    def test_mcp_errors_missing_duplicate_and_invalid_protocol_fail(self):
        good = self.replies()
        for replies in [good.splitlines()[0], good + "\n" + good.splitlines()[1],
                        good.replace('"id": 1', '"id": true'), good.replace("2025-06-18", "unknown"),
                        good.replace('"result": {"tools": [{"name": "get_node"}]}', '"error": {"code": -1}')]:
            with self.subTest(replies=replies), patch.object(catalog, "run_cli", side_effect=["figcraft-cli 1", "[]", replies]):
                with self.assertRaises(catalog.ProbeError):
                    catalog.probe("cli", 1)

    def test_release_is_one_fixed_metadata_request_without_credentials(self):
        def opened(request, timeout):
            self.assertEqual(request.full_url, catalog.RELEASE_URL)
            self.assertEqual(request.get_method(), "GET")
            self.assertIsNone(request.get_header("Authorization"))
            self.assertEqual(timeout, 2)
            return io.BytesIO(json.dumps({"tag_name": "v42", "html_url": "https://github.com/storytold/figcraft/releases/tag/v42", "assets": [{"browser_download_url": "https://invalid/never-read"}]}).encode())
        self.assertEqual(catalog.latest_release(2, opened)["tag"], "v42")

    def test_release_offline_rate_limit_oversize_and_wrong_origin_fail(self):
        def offline(*args, **kwargs):
            raise urllib.error.URLError("synthetic private detail")
        def limited(*args, **kwargs):
            raise urllib.error.HTTPError(catalog.RELEASE_URL, 429, "limited", {}, None)
        for opened in [offline, limited, lambda *a, **k: io.BytesIO(b"x" * (catalog.MAX_RELEASE + 1)),
                       lambda *a, **k: io.BytesIO(b'{"tag_name":"v1","html_url":"https://invalid/v1"}')]:
            with self.subTest(opened=opened):
                with self.assertRaises(catalog.ProbeError) as error:
                    catalog.latest_release(1, opened)
                self.assertNotIn("synthetic private detail", str(error.exception))
        self.assertIsNone(catalog.NoRedirect().redirect_request(None, None, 302, "", {}, "https://invalid"))

    def test_release_stream_has_deadline(self):
        with patch.object(catalog.time, "monotonic", side_effect=[0, 2]):
            with self.assertRaisesRegex(catalog.ProbeError, "timed out"):
                catalog.latest_release(1, lambda *a, **k: io.BytesIO(b"{}"))

    @unittest.skipIf(os.name == "nt", "synthetic executable uses a POSIX shebang")
    def test_real_child_timeout_output_cap_exit_redaction_and_reaping(self):
        with tempfile.TemporaryDirectory() as directory:
            script = Path(directory) / "fake cli"
            def executable(body):
                script.write_text("#!" + sys.executable + "\n" + body)
                script.chmod(0o700)
            executable("print('literal result')\n")
            self.assertEqual(catalog.run_cli(str(script), [";no shell"], 1), "literal result\n")
            executable("import sys; print('synthetic-secret',file=sys.stderr);sys.exit(3)\n")
            with self.assertRaises(catalog.ProbeError) as error:
                catalog.run_cli(str(script), [], 1)
            self.assertNotIn("synthetic-secret", str(error.exception))
            executable("import os,time;open(" + repr(str(Path(directory) / "pid")) + ",'w').write(str(os.getpid()));time.sleep(10)\n")
            with self.assertRaisesRegex(catalog.ProbeError, "timed out"):
                catalog.run_cli(str(script), [], 0.1)
            pid = int((Path(directory) / "pid").read_text())
            with self.assertRaises(ProcessLookupError):
                os.kill(pid, 0)
            executable("import sys;sys.stdout.write('x'*5000000)\n")
            with self.assertRaisesRegex(catalog.ProbeError, "output exceeded"):
                catalog.run_cli(str(script), [], 2)

    def test_missing_cli_and_invalid_timeout_fail_clearly(self):
        with self.assertRaisesRegex(catalog.ProbeError, "cannot start"):
            catalog.run_cli("/nonexistent/figcraft-skill-cli", [], 1)
        with contextlib.redirect_stderr(io.StringIO()), self.assertRaises(SystemExit) as error:
            catalog.main(["probe", "--timeout", "nan"])
        self.assertEqual(error.exception.code, 2)


if __name__ == "__main__":
    unittest.main()
