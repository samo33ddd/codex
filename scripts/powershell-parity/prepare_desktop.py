"""Build a local desktop with Git activity labels and its verified custom backend."""

import argparse
import hashlib
import json
import shutil
import struct
import subprocess
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
BACKEND_FILES = (
    "codex.exe",
    "codex-command-runner.exe",
    "codex-windows-sandbox-setup.exe",
    "codex-code-mode-host.exe",
)


def digest(path):
    result = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            result.update(chunk)
    return result.hexdigest()


def verified_backend_files(backend_dir):
    manifest = json.loads((backend_dir / "manifest.json").read_text(encoding="utf-8"))
    expected = {entry["file"]: entry for entry in manifest["files"]}
    files = []
    for name in BACKEND_FILES:
        path = backend_dir / name
        actual_hash = digest(path)
        if name not in expected or actual_hash != expected[name]["sha256"]:
            raise ValueError(f"Backend manifest SHA-256 mismatch: {name}")
        files.append(
            {"file": name, "bytes": path.stat().st_size, "sha256": actual_hash}
        )
    compatibility = json.loads(
        (HERE / "compatibility.json").read_text(encoding="utf-8")
    )
    version = subprocess.check_output(
        [str(backend_dir / "codex.exe"), "--version"], text=True
    ).strip()
    if version != f"codex-cli {compatibility['backendVersion']}":
        raise ValueError(f"Unexpected backend version: {version}")
    return {"version": compatibility["backendVersion"], "files": files}


def archive_header(path):
    with path.open("rb") as stream:
        size_size, header_size, payload_size, json_size = struct.unpack(
            "<4I", stream.read(16)
        )
        if size_size != 4 or header_size != payload_size + 4:
            raise ValueError("Unsupported ASAR header")
        if not 0 < json_size <= payload_size - 4 <= 32 * 1024 * 1024:
            raise ValueError("Invalid ASAR header length")
        return json.loads(stream.read(json_size)), 8 + header_size


def archive_files(header, prefix=""):
    for name, item in header["files"].items():
        full_name = f"{prefix}{name}"
        if "files" in item:
            yield from archive_files(item, full_name + "/")
        elif "offset" in item and not item.get("unpacked"):
            yield full_name, item


def rewrite_archive(source, destination, replacements):
    header, data_start = archive_header(source)
    entries = list(archive_files(header))
    missing = replacements.keys() - {name for name, _ in entries}
    if missing:
        raise ValueError(f"Missing desktop assets: {sorted(missing)}")
    original_locations = []
    offset = 0
    for name, item in entries:
        original_locations.append((int(item["offset"]), item["size"]))
        if name in replacements:
            data = replacements[name]
            item["size"] = len(data)
            integrity = item.get("integrity")
            if integrity:
                if integrity["algorithm"] != "SHA256":
                    raise ValueError("Unsupported ASAR integrity algorithm")
                block_size = integrity["blockSize"]
                integrity["hash"] = hashlib.sha256(data).hexdigest()
                integrity["blocks"] = [
                    hashlib.sha256(data[pos : pos + block_size]).hexdigest()
                    for pos in range(0, len(data), block_size)
                ]
        item["offset"] = str(offset)
        offset += item["size"]
    encoded = json.dumps(header, ensure_ascii=False, separators=(",", ":")).encode()
    payload = struct.pack("<I", len(encoded)) + encoded
    payload += b"\0" * (-len(payload) % 4)
    packed_header = struct.pack("<I", len(payload)) + payload
    with source.open("rb") as original, destination.open("xb") as patched:
        patched.write(struct.pack("<II", 4, len(packed_header)))
        patched.write(packed_header)
        for (name, _), (old_offset, old_size) in zip(
            entries, original_locations, strict=True
        ):
            if name in replacements:
                patched.write(replacements[name])
            else:
                original.seek(data_start + old_offset)
                remaining = old_size
                while remaining:
                    chunk = original.read(min(remaining, 1024 * 1024))
                    if not chunk:
                        raise ValueError("Truncated ASAR contents")
                    patched.write(chunk)
                    remaining -= len(chunk)


def prepare(source, output, backend_dir):
    source, output = source.resolve(), output.resolve()
    if output.exists() or source == output or source in output.parents:
        raise ValueError("Use a new output directory outside the installed application")
    backend_dir = backend_dir.resolve()
    backend = verified_backend_files(backend_dir)
    archive = source / "resources/app.asar"
    source_hash = digest(archive)
    patcher = HERE / "desktop_git_labels.cjs"
    names = json.loads(
        subprocess.check_output(["node", str(patcher), "--list-assets"], text=True)
    )
    header, data_start = archive_header(archive)
    entries = dict(archive_files(header))
    with tempfile.TemporaryDirectory(prefix="codex-desktop-assets-") as temporary:
        temporary = Path(temporary)
        original_dir, patched_dir = temporary / "original", temporary / "patched"
        original_dir.mkdir()
        with archive.open("rb") as stream:
            for name in names:
                if Path(name).name != name:
                    raise ValueError("Asset names must be basenames")
                item = entries[f"webview/assets/{name}"]
                stream.seek(data_start + int(item["offset"]))
                (original_dir / name).write_bytes(stream.read(item["size"]))
        subprocess.run(
            ["node", str(patcher), str(original_dir), str(patched_dir)], check=True
        )
        changes = json.loads(
            (patched_dir / "desktop_git_labels_manifest.json").read_text()
        )
        replacements = {
            f"webview/assets/{name}": (patched_dir / name).read_bytes()
            for name in names
        }
        print(f"Copying desktop to {output}", flush=True)
        shutil.copytree(source, output)
        for entry in backend["files"]:
            destination = output / "resources" / entry["file"]
            shutil.copyfile(backend_dir / entry["file"], destination)
            if digest(destination) != entry["sha256"]:
                raise ValueError(f"Copied backend SHA-256 mismatch: {entry['file']}")
        new_archive = output / "resources/app.asar.new"
        rewrite_archive(archive, new_archive, replacements)
        new_archive.replace(output / "resources/app.asar")
    if digest(archive) != source_hash:
        raise ValueError("Installed application changed during packaging")
    manifest = {
        "sourceApp": str(source),
        "sourceAsarSha256": source_hash,
        "patchedAsarSha256": digest(output / "resources/app.asar"),
        "executableSha256": digest(output / "ChatGPT.exe"),
        "assets": changes["files"],
        "backend": backend,
        "scope": "Local desktop and backend bundle; installed application is unchanged",
    }
    (output / "desktop-patch-manifest.json").write_text(
        json.dumps(manifest, indent=2) + "\n", encoding="utf-8"
    )
    return manifest


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source-app", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--backend-dir", type=Path, required=True)
    args = parser.parse_args()
    print(json.dumps(prepare(args.source_app, args.output, args.backend_dir), indent=2))
