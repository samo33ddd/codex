import hashlib
import json
import struct
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from prepare_desktop import BACKEND_FILES, HERE, prepare, verified_backend_files


class DesktopBackendTests(unittest.TestCase):
    def test_successful_preparation_registers_latest_bundle_for_default_launcher(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            source, backend = root / "installed", root / "backend"
            (source / "resources").mkdir(parents=True)
            backend.mkdir()
            (source / "ChatGPT.exe").write_bytes(b"desktop fixture")
            (backend / "codex.exe").write_bytes(b"custom backend fixture")
            files = [
                {
                    "file": "codex.exe",
                    "sha256": hashlib.sha256(b"custom backend fixture").hexdigest(),
                }
            ]
            header = {
                "files": {
                    "webview": {
                        "files": {
                            "assets": {
                                "files": {"fixture.js": {"offset": "0", "size": 8}}
                            }
                        }
                    }
                }
            }
            encoded = json.dumps(header).encode()
            payload = struct.pack("<I", len(encoded)) + encoded
            payload += b"\0" * (-len(payload) % 4)
            archive = (
                struct.pack("<III", 4, len(payload) + 4, len(payload))
                + payload
                + b"original"
            )
            (source / "resources/app.asar").write_bytes(archive)

            def patch_assets(command, *, check):
                original, patched = Path(command[-2]), Path(command[-1])
                self.assertEqual((original / "fixture.js").read_bytes(), b"original")
                patched.mkdir()
                (patched / "fixture.js").write_bytes(b"patched")
                (patched / "desktop_git_labels_manifest.json").write_text(
                    '{"files": []}'
                )

            registration = backend / "desktop-bundle.json"
            with (
                patch(
                    "prepare_desktop.verified_backend_files",
                    return_value={"files": files},
                ),
                patch(
                    "prepare_desktop.subprocess.check_output",
                    return_value='["fixture.js"]',
                ),
                patch("prepare_desktop.subprocess.run", side_effect=patch_assets),
            ):
                for name in ("first bundle", "second bundle"):
                    output = root / name
                    prepare(source, output, backend)
                    self.assertEqual(
                        json.loads(registration.read_text()),
                        {"desktopPath": str(output / "ChatGPT.exe")},
                    )
                    self.assertTrue((output / "desktop-patch-manifest.json").is_file())
                with self.assertRaisesRegex(ValueError, "new output directory"):
                    prepare(source, source, backend)
                self.assertEqual(
                    json.loads(registration.read_text()),
                    {"desktopPath": str(root / "second bundle/ChatGPT.exe")},
                )
                with (
                    patch(
                        "prepare_desktop.shutil.copytree",
                        side_effect=OSError("disk full"),
                    ),
                    self.assertRaisesRegex(OSError, "disk full"),
                ):
                    prepare(source, root / "failed update", backend)
                self.assertEqual(
                    json.loads(registration.read_text()),
                    {"desktopPath": str(root / "second bundle/ChatGPT.exe")},
                )
                with (
                    patch("prepare_desktop.subprocess.check_output", return_value="[]"),
                    patch("prepare_desktop.subprocess.run") as patcher,
                ):
                    output = root / "future desktop"
                    manifest = prepare(source, output, backend)
                    patcher.assert_not_called()
                    self.assertEqual(manifest["gitLabels"], "stock")
                    self.assertEqual(manifest["assets"], [])
                    self.assertEqual(
                        (output / "resources/app.asar").read_bytes(), archive
                    )
                    self.assertEqual(
                        (output / "resources/codex.exe").read_bytes(),
                        b"custom backend fixture",
                    )
                    self.assertEqual(
                        json.loads(registration.read_text()),
                        {"desktopPath": str(output / "ChatGPT.exe")},
                    )
            self.assertEqual((source / "resources/app.asar").read_bytes(), archive)

    def test_bundle_verifies_every_component_before_using_it(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            entries = []
            for name in BACKEND_FILES:
                data = f"custom fixture for {name}".encode()
                (root / name).write_bytes(data)
                entries.append(
                    {
                        "file": name,
                        "bytes": len(data),
                        "sha256": hashlib.sha256(data).hexdigest(),
                    }
                )
            (root / "manifest.json").write_text(json.dumps({"files": entries}))
            version = json.loads((HERE / "compatibility.json").read_text())[
                "backendVersion"
            ]
            with patch(
                "prepare_desktop.subprocess.check_output",
                return_value=f"codex-cli {version}\n",
            ) as run:
                self.assertEqual(
                    verified_backend_files(root), {"version": version, "files": entries}
                )
                run.assert_called_once_with(
                    [str(root / "codex.exe"), "--version"], text=True
                )
            for entry in entries:
                component = root / entry["file"]
                original = component.read_bytes()
                component.write_bytes(b"stock component substituted")
                with (
                    self.subTest(component=entry["file"]),
                    patch("prepare_desktop.subprocess.check_output") as run,
                ):
                    with self.assertRaisesRegex(ValueError, "SHA-256 mismatch"):
                        verified_backend_files(root)
                    run.assert_not_called()
                component.write_bytes(original)
            with (
                patch(
                    "prepare_desktop.subprocess.check_output",
                    return_value="codex-cli 0.0.0",
                ),
                self.assertRaisesRegex(ValueError, "Unexpected backend version"),
            ):
                verified_backend_files(root)


if __name__ == "__main__":
    unittest.main()
