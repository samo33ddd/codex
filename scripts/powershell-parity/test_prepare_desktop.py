import hashlib
import json
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from prepare_desktop import BACKEND_FILES, HERE, verified_backend_files


class DesktopBackendTests(unittest.TestCase):
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
