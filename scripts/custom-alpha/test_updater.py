#!/usr/bin/env python3
"""Focused offline checks for the custom alpha updater."""

import json
import base64
import hashlib
import io
import sys
import tarfile
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import update


class FakeOps:
    def __init__(self, root, idle=True, current=False, failure=None):
        self.events, self.idle, self.current, self.failure = [], idle, current, failure
        self.root = Path(root)

    def prepared(self):
        self.events.append("prepared")
        return None

    def latest(self):
        self.events.append("latest")
        return "1.2.3-alpha.4"

    def already_current(self, version):
        self.events.append("current")
        return self.current

    def build_candidate(self, version):
        self.events.append("stage")
        if self.failure:
            raise self.failure
        return self.root / version

    def safe(self):
        self.events.append("idle")
        return self.idle

    def activate(self, candidate):
        self.events.append("activate")


def main():
    assert update.choose_alpha({"dist-tags": {"alpha": "1.2.3-alpha.4"}, "versions": {"1.2.3-alpha.4": {}}}) == "1.2.3-alpha.4"
    for bad in ("v1.2.3-alpha.4", "1.02.3-alpha.4", "1.2.3-beta.4", "1.2.3-alpha.04"):
        try:
            update.choose_alpha({"dist-tags": {"alpha": bad}, "versions": {bad: {}}})
            raise AssertionError(f"accepted invalid alpha {bad}")
        except update.UpdateError:
            pass

    with tempfile.TemporaryDirectory() as folder:
        cargo = Path(folder) / "Cargo.toml"
        good = '[workspace.package]\n<<<<<<< ours\nversion = "1.0.0-alpha.1"\n=======\nversion = "1.2.3-alpha.4"\n>>>>>>> tag\n'
        cargo.write_text(good, encoding="utf-8")
        update.resolve_version_conflict(cargo, "1.2.3-alpha.4")
        assert cargo.read_text(encoding="utf-8") == '[workspace.package]\nversion = "1.2.3-alpha.4"\n'

        bad = '[workspace.package]\n<<<<<<< ours\nversion = "1.0.0-alpha.1"\n=======\nversion = "1.2.3-alpha.4"\n>>>>>>> tag\n[package]\n<<<<<<< ours\nname = "a"\n=======\nname = "b"\n>>>>>>> tag\n'
        cargo.write_text(bad, encoding="utf-8")
        try:
            update.resolve_version_conflict(cargo, "1.2.3-alpha.4")
            raise AssertionError("accepted non-version conflict")
        except update.UpdateError:
            assert cargo.read_text(encoding="utf-8") == bad

        installed = Path(folder) / "installed.exe"
        installed.write_bytes(b"last-known-good")
        ops = FakeOps(folder, failure=update.UpdateError("conflict", "other conflict"))
        assert update.run_pipeline(ops, Path(folder) / "failed") == 1
        assert installed.read_bytes() == b"last-known-good"
        assert "activate" not in ops.events

        ops = FakeOps(folder, idle=False)
        assert update.run_pipeline(ops, Path(folder) / "deferred") == 0
        assert ops.events == ["prepared", "latest", "current", "stage", "idle"]

        ops = FakeOps(folder)
        assert update.run_pipeline(ops, Path(folder) / "success") == 0
        assert ops.events == ["prepared", "latest", "current", "stage", "idle", "activate"]
        assert json.loads((Path(folder) / "success" / "status.json").read_text())["state"] == "activated"

        ops = FakeOps(folder, current=True)
        assert update.run_pipeline(ops, Path(folder) / "current") == 0
        assert ops.events == ["prepared", "latest", "current"]
        assert json.loads((Path(folder) / "current" / "status.json").read_text())["state"] == "current"

        version = "0.158.0-alpha.8"
        platform_version = version + "-win32-x64"
        packed = io.BytesIO()
        with tarfile.open(fileobj=packed, mode="w:gz") as archive:
            files = {
                "package/package.json": json.dumps({"name": "@openai/codex", "version": platform_version}).encode(),
                "package/vendor/x86_64-pc-windows-msvc/codex-package.json": b"{}",
                "package/vendor/x86_64-pc-windows-msvc/bin/codex.exe": b"official exe",
                "package/vendor/x86_64-pc-windows-msvc/codex-resources/resource": b"resource",
                "package/vendor/x86_64-pc-windows-msvc/codex-path": b"helper",
            }
            for name, data in files.items():
                info = tarfile.TarInfo(name)
                info.size = len(data)
                archive.addfile(info, io.BytesIO(data))
        blob = packed.getvalue()
        integrity = "sha512-" + base64.b64encode(hashlib.sha512(blob).digest()).decode()
        metadata = {"versions": {platform_version: {"name": "@openai/codex", "version": platform_version,
                     "dist": {"tarball": "https://registry.test/package.tgz", "integrity": integrity}}}}
        original_https = update._https
        update._https = lambda url, limit: json.dumps(metadata).encode() if "registry.npmjs.org" in url else blob
        try:
            ops = update.Ops({"sourceRepository": folder, "scratchWorktreeRoot": str(Path(folder) / "worktree"),
                              "artifactsRoot": str(Path(folder) / "artifacts"),
                              "officialPackageTemplatePath": str(Path(folder) / "templates")})
            template, _ = ops._template(version)
            assert (template / "bin" / "codex.exe").read_bytes() == b"official exe"
            assert (template / "codex-resources" / "resource").read_bytes() == b"resource"
            assert not (template / "package.json").exists()
        finally:
            update._https = original_https

        home = Path(folder) / "codex-home"
        release_exe = home / "packages" / "app-server-daemon" / "releases" / "local-HASH-x86_64-pc-windows-msvc" / "bin" / "codex.exe"
        release_exe.parent.mkdir(parents=True)
        release_exe.write_bytes(b"managed daemon")
        daemon_paths = {update._windows_path_key(release_exe): str(home)}
        extended = "\\\\?\\" + str(release_exe)
        fixture = {"ExecutablePath": extended,
                   "CommandLine": f'"{extended}" app-server --listen unix:// --managed-daemon'}
        assert update._managed_daemon_home(fixture, daemon_paths) == str(home)
        assert update._managed_daemon_home({**fixture, "CommandLine": f'"{extended}" app-server --listen unix://'}, daemon_paths) is None
        assert update._managed_daemon_home({**fixture, "ExecutablePath": str(Path(folder) / "codex.exe")}, daemon_paths) is None

    print("custom-alpha updater checks passed")


if __name__ == "__main__":
    main()
