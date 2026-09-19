"""Package locally built Windows binaries with a pinned official code-mode host."""

import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import urllib.request
import zipfile


HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
BINARIES = ("codex.exe", "codex-command-runner.exe", "codex-windows-sandbox-setup.exe")


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def verified_host(destination, source, cached=None):
    if cached is not None:
        if digest(cached) != source["sha256"]:
            raise ValueError(
                "Cached code-mode-host SHA-256 does not match the pinned official asset"
            )
        shutil.copyfile(cached, destination)
    else:
        with urllib.request.urlopen(source["url"], timeout=120) as response:
            with destination.open("wb") as stream:
                shutil.copyfileobj(response, stream)
        if digest(destination) != source["sha256"]:
            destination.unlink()
            raise ValueError(
                "Downloaded code-mode-host SHA-256 mismatch; package rejected"
            )


def package(args):
    compatibility = json.loads(
        (HERE / "compatibility.json").read_text(encoding="utf-8")
    )
    binary_dir = args.binary_dir.resolve()
    output = args.output.resolve()
    if output.exists() and any(output.iterdir()):
        raise ValueError(
            "Output must be a new or empty directory; existing packages are never overwritten"
        )
    for name in BINARIES:
        if not (binary_dir / name).is_file():
            raise FileNotFoundError(binary_dir / name)
    version = subprocess.check_output(
        [str(binary_dir / "codex.exe"), "--version"], text=True
    ).strip()
    if version != f"codex-cli {compatibility['backendVersion']}":
        raise ValueError(f"Unexpected backend version: {version}")
    output.mkdir(parents=True, exist_ok=True)
    for name in BINARIES:
        shutil.copyfile(binary_dir / name, output / name)
    verified_host(
        output / "codex-code-mode-host.exe",
        compatibility["codeModeHost"],
        args.host_file,
    )
    for name in (
        "Start-Codex.ps1",
        "compatibility.json",
        "prepare_desktop.py",
        "desktop_git_labels.cjs",
    ):
        shutil.copyfile(HERE / name, output / name)
    for name in ("LICENSE", "NOTICE"):
        shutil.copyfile(ROOT / name, output / name)
    manifest = {
        "sourceCommit": subprocess.check_output(
            ["git", "rev-parse", "HEAD"], cwd=ROOT, text=True
        ).strip(),
        "buildProfile": args.profile,
        "compatibility": compatibility,
        "desktopAcceptance": "Required for each published package; CI does not test the desktop UI",
        "files": [
            {"file": path.name, "bytes": path.stat().st_size, "sha256": digest(path)}
            for path in sorted(output.iterdir())
            if path.is_file()
        ],
    }
    (output / "manifest.json").write_text(
        json.dumps(manifest, indent=2) + "\n", encoding="utf-8"
    )
    archive = output.with_suffix(".zip")
    if archive.exists():
        raise FileExistsError(archive)
    with zipfile.ZipFile(archive, "x", compression=zipfile.ZIP_DEFLATED) as bundle:
        for path in sorted(output.iterdir()):
            bundle.write(path, path.name)
    archive.with_suffix(".zip.sha256").write_text(
        f"{digest(archive)}  {archive.name}\n", encoding="utf-8"
    )
    print(archive)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary-dir", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--profile", choices=("dev", "release"), required=True)
    parser.add_argument(
        "--host-file",
        type=Path,
        help="Already downloaded official asset; hash is still verified",
    )
    package(parser.parse_args())
