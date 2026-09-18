import hashlib
from pathlib import Path
import subprocess
import tempfile
import unittest

from check_upstream import check_patch
from package_windows import verified_host


class DeliveryTests(unittest.TestCase):
    def test_host_hash_rejects_wrong_component_before_copy(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            host = root / "host.exe"
            host.write_bytes(b"wrong component")
            destination = root / "package.exe"
            with self.assertRaisesRegex(ValueError, "SHA-256"):
                verified_host(destination, {"sha256": "0" * 64}, host)
            self.assertFalse(destination.exists())

    def test_host_preserves_verified_bytes(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            host = root / "host.exe"
            host.write_bytes(b"verified fixture")
            destination = root / "package.exe"
            verified_host(
                destination,
                {"sha256": hashlib.sha256(host.read_bytes()).hexdigest()},
                host,
            )
            self.assertEqual(destination.read_bytes(), host.read_bytes())

    def test_patch_check_preserves_checkout_and_detects_three_outcomes(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)

            def command(*args):
                return subprocess.check_output(
                    ["git", "-C", str(root), *args], text=True
                ).strip()

            def commit(text):
                (root / "codex-rs/parser.txt").write_text(text, encoding="utf-8")
                command("add", ".")
                command(
                    "-c",
                    "user.name=Fixture",
                    "-c",
                    "user.email=fixture@example.invalid",
                    "commit",
                    "-m",
                    text,
                )
                return command("rev-parse", "HEAD")

            command("init", "--quiet")
            (root / "codex-rs").mkdir()
            base = commit("baseline\n")
            patched = commit("patched\n")
            divergent = commit("upstream changed\n")
            before = command("status", "--porcelain=v1")
            self.assertEqual(
                check_patch(root, base, patched, base)["status"], "applies"
            )
            self.assertEqual(
                check_patch(root, base, patched, patched)["status"], "already-applied"
            )
            self.assertEqual(
                check_patch(root, base, patched, divergent)["status"], "conflict"
            )
            self.assertEqual(command("rev-parse", "HEAD"), divergent)
            self.assertEqual(command("status", "--porcelain=v1"), before)
            self.assertEqual(
                (root / "codex-rs/parser.txt").read_text(encoding="utf-8"),
                "upstream changed\n",
            )


if __name__ == "__main__":
    unittest.main()
