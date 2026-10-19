#!/usr/bin/env python3
"""Exercise the Codex installer using temporary configuration files only."""

import os
from pathlib import Path
import stat
import subprocess
import sys
import tempfile
import tomllib
import unittest


ROOT = Path(__file__).resolve().parents[1]
INSTALLER = ROOT / "scripts" / "install-codex.py"
LAUNCHER = str(ROOT / "scripts" / "jrg-mcp.sh")


class CodexInstallTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory(prefix="jrg-codex-install-")
        self.addCleanup(self.directory.cleanup)
        self.home = Path(self.directory.name)
        self.config = self.home / "config.toml"

    def run_install(self, *arguments, default=False):
        command = [sys.executable, "-B", str(INSTALLER)]
        if not default:
            command.extend(["--config", str(self.config)])
        return subprocess.run(command + list(arguments), capture_output=True, text=True,
                              env={**os.environ, "HOME": str(self.home)})

    def assert_success(self, result):
        self.assertEqual(result.returncode, 0, result.stderr)

    def assert_jrg(self, path):
        data = tomllib.loads(path.read_text())
        server = data["mcp_servers"]["jrg"]
        self.assertEqual(server["command"], "/bin/sh")
        self.assertEqual(server["args"], [LAUNCHER])
        self.assertNotIn("cwd", server)
        self.assertEqual(server["startup_timeout_sec"], 10)
        self.assertEqual(server["tool_timeout_sec"], 180)
        self.assertEqual(server["env"]["JRG_EMBEDDING_ENABLED"], "1")
        self.assertEqual(server["tools"]["retrieve"]["output_token_limit"], 30000)
        return data

    def backups(self, path=None):
        path = path or self.config
        return list(path.parent.glob("." + path.name + ".jrg-backup-*"))

    def test_effective_configuration_preserves_unrelated_text_and_values(self):
        original = ('# Keep this exact comment.\nmodel = "fixture-model"\n'
                    '[features]\nflag = true # keep me\n'
                    '[mcp_servers.other]\ncommand = "fixture-secret"\n'
                    'args = ["--key", "fixture-secret"]\n')
        self.config.write_text(original)
        result = self.run_install()
        self.assert_success(result)
        data = self.assert_jrg(self.config)
        self.assertEqual(data["model"], "fixture-model")
        self.assertTrue(data["features"]["flag"])
        self.assertEqual(data["mcp_servers"]["other"]["command"], "fixture-secret")
        root_text, table_text = original.split("[features]", 1)
        self.assertIn(root_text, self.config.read_text())
        self.assertIn("[features]" + table_text, self.config.read_text())
        instructions = data["developer_instructions"]
        for phrase in ("jrg", "retrieve", "working directory", "native `rg`",
                       "exhaustive", "outside", "Never replace"):
            self.assertIn(phrase, instructions)
        self.assertNotIn("fixture-secret", result.stdout + result.stderr)

    def test_preserves_existing_instructions_and_multiline_decoys(self):
        original = ('# instruction comment\ndeveloper_instructions = \'\'\'Keep this instruction.\n'
                    '[mcp_servers.jrg]\ncommand = "a quoted example"\n\'\'\' # inline comment\n'
                    '[features]\nkeep = true\n')
        self.config.write_text(original)
        previous = tomllib.loads(original)["developer_instructions"]
        self.assert_success(self.run_install())
        actual = self.assert_jrg(self.config)
        self.assertTrue(actual["developer_instructions"].startswith(previous))
        self.assertIn("# instruction comment", self.config.read_text())
        self.assertIn("# inline comment", self.config.read_text())
        self.assertTrue(actual["features"]["keep"])

    def test_reinstall_is_byte_and_backup_idempotent(self):
        self.config.write_text('developer_instructions = "Keep exactly.\\n"\n')
        self.assert_success(self.run_install())
        first = self.config.read_bytes()
        modified = self.config.stat().st_mtime_ns
        backups = self.backups()
        self.assertEqual(len(backups), 1)
        self.assert_success(self.run_install())
        self.assertEqual(self.config.read_bytes(), first)
        self.assertEqual(self.config.stat().st_mtime_ns, modified)
        self.assertEqual(self.backups(), backups)
        self.assertEqual(tomllib.loads(first.decode())["developer_instructions"].count(
            "<!-- jrg-codex:begin -->"), 1)

    def test_migrates_fixed_project_override_preserving_other_settings(self):
        self.config.write_text('model = "global"\n')
        project = self.home / "project.toml"
        project.write_text('# project comment\n[mcp_servers.jrg]\ncommand = "/bin/sh"\n'
                           'args = ["/old/jrg-mcp.sh", "/old/fixed/root"]\ncwd = "/old/root"\n'
                           'enabled = true\n[mcp_servers.jrg.env]\n'
                           'JRG_EMBEDDING_ENABLED = "0"\nOTHER = "retain"\n'
                           '[mcp_servers.jrg.tools.retrieve]\noutput_token_limit = 5\n'
                           '[mcp_servers.other]\ncommand = "untouched"\n')
        self.assert_success(self.run_install("--project-config", str(project)))
        data = self.assert_jrg(project)
        self.assertTrue(data["mcp_servers"]["jrg"]["enabled"])
        self.assertEqual(data["mcp_servers"]["jrg"]["env"]["OTHER"], "retain")
        self.assertEqual(data["mcp_servers"]["other"]["command"], "untouched")
        self.assertNotIn("developer_instructions", data)
        self.assertEqual(len(self.backups(project)), 1)

    def test_project_without_override_is_not_created_or_modified(self):
        project = self.home / "project.toml"
        self.assert_success(self.run_install("--project-config", str(project)))
        self.assertFalse(project.exists())
        project.write_text('# no local jrg\nmodel = "local"\n')
        before = project.read_bytes()
        self.assert_success(self.run_install("--project-config", str(project)))
        self.assertEqual(project.read_bytes(), before)
        self.assertFalse(self.backups(project))

    def test_dry_run_changes_nothing_and_creates_no_directories(self):
        self.config.write_text('model = "fixture-secret"\n')
        original = self.config.read_bytes()
        self.assert_success(self.run_install("--dry-run"))
        self.assertEqual(self.config.read_bytes(), original)
        self.assertFalse(self.backups())
        missing = self.home / "missing" / "config.toml"
        result = self.run_install("--config", str(missing), "--dry-run")
        self.assert_success(result)
        self.assertFalse(missing.parent.exists())
        self.assertNotIn("fixture-secret", result.stdout + result.stderr)

    def test_malformed_input_is_untouched_before_any_configuration_write(self):
        self.config.write_text('model = "valid"\n')
        project = self.home / "project.toml"
        project.write_text('malformed = ["fixture-secret"\n')
        before = {p: p.read_bytes() for p in (self.config, project)}
        result = self.run_install("--project-config", str(project))
        self.assertNotEqual(result.returncode, 0)
        for path, content in before.items():
            self.assertEqual(path.read_bytes(), content)
            self.assertFalse(self.backups(path))
        self.assertNotIn("fixture-secret", result.stdout + result.stderr)

    def test_backup_is_exact_and_private_and_original_mode_is_preserved(self):
        original = b'model = "private-fixture"\r\n# preserve CRLF\r\n'
        self.config.write_bytes(original)
        self.config.chmod(0o640)
        self.assert_success(self.run_install())
        self.assertEqual(stat.S_IMODE(self.config.stat().st_mode), 0o640)
        backup, = self.backups()
        self.assertEqual(backup.read_bytes(), original)
        self.assertEqual(stat.S_IMODE(backup.stat().st_mode), 0o600)
        self.assertIn(original, self.config.read_bytes())

    def test_default_path_and_new_file_are_private(self):
        self.assert_success(self.run_install(default=True))
        path = self.home / ".codex" / "config.toml"
        self.assert_jrg(path)
        self.assertEqual(stat.S_IMODE(path.stat().st_mode), 0o600)
        self.assertFalse(self.backups(path))

    def test_ambiguous_inline_target_is_rejected_without_changes(self):
        original = 'mcp_servers = { jrg = { command = "fixture-secret" } }\n'
        self.config.write_text(original)
        result = self.run_install()
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(self.config.read_text(), original)
        self.assertFalse(self.backups())
        self.assertNotIn("fixture-secret", result.stdout + result.stderr)

    def test_dotted_managed_keys_and_quoted_headers_are_updated(self):
        self.config.write_text('[mcp_servers."jrg"]\ncommand = "old"\n'
                               'env.JRG_EMBEDDING_ENABLED = "0"\n'
                               'tools.retrieve.output_token_limit = 1\n')
        self.assert_success(self.run_install())
        self.assert_jrg(self.config)

    def test_inline_environment_migration_preserves_unrelated_string_values(self):
        project = self.home / "project.toml"
        for embedding in ('JRG_EMBEDDING_ENABLED = "1", ', "", 'JRG_EMBEDDING_ENABLED = "0", '):
            with self.subTest(embedding=embedding):
                self.config.write_text('sandbox_workspace_write = { network_access = true }\n'
                                       '[mcp_servers.other]\nenv = { KEY = "fixture-secret" }\n')
                project.write_text('[mcp_servers.jrg]\ncommand = "/bin/sh"\n'
                                   'args = ["/old/jrg-mcp.sh", "/old/fixed/root"]\n'
                                   'env = { ' + embedding +
                                   'OTHER = "retain # } \\\"quoted\\\"" } # retain comment\n')
                result = self.run_install("--project-config", str(project))
                self.assert_success(result)
                data = self.assert_jrg(project)
                self.assertEqual(data["mcp_servers"]["jrg"]["env"]["OTHER"],
                                 'retain # } "quoted"')
                self.assertIn("# retain comment", project.read_text())
                global_data = self.assert_jrg(self.config)
                self.assertTrue(global_data["sandbox_workspace_write"]["network_access"])
                self.assertEqual(global_data["mcp_servers"]["other"]["env"]["KEY"], "fixture-secret")
                self.assertNotIn("fixture-secret", result.stdout + result.stderr)
                installed = project.read_bytes()
                backup_count = len(self.backups(project))
                self.assert_success(self.run_install("--project-config", str(project)))
                self.assertEqual(project.read_bytes(), installed)
                self.assertEqual(len(self.backups(project)), backup_count)

    def test_symlink_and_unbalanced_instruction_marker_are_rejected(self):
        original = self.home / "original.toml"
        original.write_text('model = "retain"\n')
        self.config.symlink_to(original)
        self.assertNotEqual(self.run_install().returncode, 0)
        self.assertTrue(self.config.is_symlink())
        self.assertEqual(original.read_text(), 'model = "retain"\n')
        self.config.unlink()
        self.config.write_text('developer_instructions = "<!-- jrg-codex:begin -->"\n')
        before = self.config.read_bytes()
        self.assertNotEqual(self.run_install().returncode, 0)
        self.assertEqual(self.config.read_bytes(), before)
        self.assertFalse(self.backups())


if __name__ == "__main__":
    unittest.main()
