#!/usr/bin/env python3
"""Focused offline checks for the custom alpha updater."""

import json
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import update


class FakeOps:
    def __init__(self, root, idle=True, failure=None):
        self.events, self.idle, self.failure = [], idle, failure
        self.root = Path(root)

    def prepared(self):
        self.events.append("prepared")
        return None

    def latest(self):
        self.events.append("latest")
        return "1.2.3-alpha.4"

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
        assert ops.events == ["prepared", "latest", "stage", "idle"]

        ops = FakeOps(folder)
        assert update.run_pipeline(ops, Path(folder) / "success") == 0
        assert ops.events == ["prepared", "latest", "stage", "idle", "activate"]
        assert json.loads((Path(folder) / "success" / "status.json").read_text())["state"] == "activated"

    print("custom-alpha updater checks passed")


if __name__ == "__main__":
    main()
