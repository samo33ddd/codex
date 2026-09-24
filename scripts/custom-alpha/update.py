#!/usr/bin/env python3
"""Build and safely offer the latest official Codex alpha to the managed daemon."""

from __future__ import annotations

import base64
import hashlib
import json
import os
import re
import shutil
import socket
import struct
import subprocess
import sys
import tarfile
import tempfile
import urllib.request
from datetime import datetime, timezone
from pathlib import Path, PurePosixPath


ALPHA = re.compile(r"^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)-alpha(?:\.(?:0|[1-9]\d*|[0-9A-Za-z-]*[A-Za-z-][0-9A-Za-z-]*))*(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?$")
MAX_TARBALL = 512 * 1024 * 1024
MAX_EXPANDED = 1024 * 1024 * 1024


class UpdateError(RuntimeError):
    def __init__(self, stage, message):
        super().__init__(message[:500])
        self.stage = stage


def choose_alpha(metadata):
    version = metadata.get("dist-tags", {}).get("alpha")
    if not isinstance(version, str) or not ALPHA.fullmatch(version):
        raise UpdateError("select", "npm alpha dist-tag is not a strict alpha semver")
    if version not in metadata.get("versions", {}):
        raise UpdateError("select", "npm alpha dist-tag has no matching package version")
    return version


def resolve_version_conflict(path, version):
    lines = Path(path).read_text(encoding="utf-8").splitlines(keepends=True)
    out, i, conflicts = [], 0, 0
    section = ""
    while i < len(lines):
        line = lines[i]
        header = re.match(r"\s*\[([^]]+)\]", line)
        if header:
            section = header.group(1)
        if not line.startswith("<<<<<<< "):
            if line.startswith(("=======", ">>>>>>> ")):
                raise UpdateError("conflict", "unexpected Cargo.toml conflict marker")
            out.append(line)
            i += 1
            continue
        conflicts += 1
        if conflicts != 1 or section != "workspace.package":
            raise UpdateError("conflict", "Cargo.toml conflict is not the sole workspace.package version conflict")
        i += 1
        sides = [[], []]
        side = 0
        while i < len(lines) and not lines[i].startswith(">>>>>>> "):
            if lines[i].startswith("======="):
                if side:
                    raise UpdateError("conflict", "malformed Cargo.toml conflict")
                side = 1
            else:
                sides[side].append(lines[i].strip())
            i += 1
        if i == len(lines) or not all(len(s) == 1 and re.fullmatch(r'version\s*=\s*"[^"\r\n]+"', s[0]) for s in sides):
            raise UpdateError("conflict", "Cargo.toml conflict contains changes beyond version")
        out.append(f'version = "{version}"\n')
        i += 1
    if conflicts != 1:
        raise UpdateError("conflict", "expected one workspace.package version conflict")
    Path(path).write_text("".join(out), encoding="utf-8", newline="")


def _run(args, cwd=None, env=None, check=True):
    p = subprocess.run(args, cwd=cwd, env=env, text=True, stdout=subprocess.PIPE,
                       stderr=subprocess.STDOUT, errors="replace")
    if check and p.returncode:
        raise UpdateError("command", f"{Path(str(args[0])).name} failed ({p.returncode}): {p.stdout[-800:]}")
    return p


def _hash(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


def _https(url, limit):
    req = urllib.request.Request(url, headers={"User-Agent": "codex-custom-alpha-updater/1"})
    with urllib.request.urlopen(req, timeout=40) as response:
        data = response.read(limit + 1)
    if len(data) > limit:
        raise UpdateError("download", "registry response exceeds size limit")
    return data


def _verify_idle(home):
    """Fail closed unless local-control proves every thread is not loaded."""
    path = str(Path(home) / "app-server-daemon" / "app-server-control.sock")
    if len(path) >= 108:
        return False
    try:
        sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        sock.settimeout(5)
        sock.connect(path)
        key = base64.b64encode(os.urandom(16)).decode("ascii")
        sock.sendall((f"GET / HTTP/1.1\r\nHost: codex.local\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n\r\n").encode())
        header = bytearray()
        while b"\r\n\r\n" not in header and len(header) < 16384:
            header.extend(sock.recv(1024))
        accept = base64.b64encode(hashlib.sha1((key + "258EAFA5-E914-47DA-95CA-C5AB0DC85B11").encode()).digest()).decode()
        response_headers = {}
        for line in header.decode("latin1").split("\r\n")[1:]:
            if ":" in line:
                name, value = line.split(":", 1)
                response_headers[name.strip().lower()] = value.strip()
        if b" 101 " not in header or response_headers.get("sec-websocket-accept") != accept:
            return False
        next_id = 0

        def send_frame(raw, opcode=1):
            mask = os.urandom(4)
            n = len(raw)
            prefix = bytes([0x80 | opcode, 0x80 | (n if n < 126 else 126 if n < 65536 else 127)])
            if 126 <= n < 65536:
                prefix += struct.pack("!H", n)
            elif n >= 65536:
                prefix += struct.pack("!Q", n)
            sock.sendall(prefix + mask + bytes(b ^ mask[i % 4] for i, b in enumerate(raw)))

        def send(message):
            send_frame(json.dumps(message, separators=(",", ":")).encode())

        def read_exact(size):
            result = bytearray()
            while len(result) < size:
                chunk = sock.recv(size - len(result))
                if not chunk:
                    raise ValueError("websocket closed")
                result.extend(chunk)
            return bytes(result)

        def receive():
            data = bytearray()
            while True:
                first, second = read_exact(2)
                op, size = first & 15, second & 127
                if size == 126:
                    size = struct.unpack("!H", read_exact(2))[0]
                elif size == 127:
                    size = struct.unpack("!Q", read_exact(8))[0]
                if size > 2 * 1024 * 1024:
                    raise ValueError("oversize websocket message")
                mask = read_exact(4) if second & 128 else b""
                chunk = bytearray(read_exact(size))
                if mask:
                    chunk = bytearray(b ^ mask[i % 4] for i, b in enumerate(chunk))
                if op == 9:
                    send_frame(chunk, 10)
                    continue
                if op == 8:
                    raise ValueError("websocket closed")
                data.extend(chunk)
                if len(data) > 2 * 1024 * 1024:
                    raise ValueError("oversize websocket message")
                if first & 128:
                    return json.loads(data)

        def rpc(method, params):
            nonlocal next_id
            next_id += 1
            send({"jsonrpc": "2.0", "id": next_id, "method": method, "params": params})
            while True:
                msg = receive()
                if str(msg.get("id")) == str(next_id):
                    if "error" in msg:
                        raise ValueError("local-control RPC error")
                    return msg["result"]

        rpc("initialize", {"clientInfo": {"name": "custom-alpha-updater", "title": "Custom alpha updater", "version": "1"},
                            "capabilities": {"experimentalApi": True, "requestAttestation": False,
                                             "mcpServerOpenaiFormElicitation": False, "extensions": {}}})
        send({"jsonrpc": "2.0", "method": "initialized", "params": {}})
        cursor, seen = None, set()
        for _ in range(100):
            params = {"limit": 100}
            if cursor:
                params["cursor"] = cursor
            result = rpc("thread/list", params)
            data = result.get("data")
            if not isinstance(data, list):
                return False
            for thread in data:
                status = thread.get("status") if isinstance(thread, dict) else None
                if isinstance(status, dict):
                    status = status.get("type")
                if status not in ("notLoaded", "not_loaded"):
                    return False
            cursor = result.get("nextCursor")
            if not cursor:
                return True
            if cursor in seen:
                return False
            seen.add(cursor)
        return False
    except Exception:
        return False
    finally:
        try:
            sock.close()
        except Exception:
            pass


class Ops:
    def __init__(self, config):
        self.c = config
        self.repo = Path(config["sourceRepository"]).resolve()
        self.wt = Path(config["scratchWorktreeRoot"]).resolve()
        self.artifacts = Path(config["artifactsRoot"]).resolve()

    def _worktree(self):
        marker = self.wt.with_suffix(".owner.json")
        owner = {"repository": str(self.repo), "sourceRef": self.c["sourceRef"]}
        self.wt.parent.mkdir(parents=True, exist_ok=True)
        if self.wt.exists():
            if not marker.exists() or json.loads(marker.read_text()) != owner:
                raise UpdateError("worktree", "scratch path exists but is not this updater's worktree")
            if _run(["git", "rev-parse", "--show-toplevel"], cwd=self.wt).stdout.strip() != str(self.wt):
                raise UpdateError("worktree", "scratch path is not a git worktree")
            if _run(["git", "status", "--porcelain"], cwd=self.wt).stdout.strip():
                raise UpdateError("worktree", "updater worktree has local changes; preserving them")
        else:
            _run(["git", "worktree", "add", "--detach", str(self.wt), self.c["sourceRef"]], cwd=self.repo)
            marker.write_text(json.dumps(owner), encoding="utf-8")
        return self.wt

    def latest(self):
        base = json.loads(_https("https://registry.npmjs.org/%40openai%2Fcodex", 32 * 1024 * 1024))
        return choose_alpha(base)

    def _merge_version(self, version):
        wt = self._worktree()
        p = _run(["git", "merge", "--no-edit", self.c["sourceRef"]], cwd=wt, check=False)
        if p.returncode:
            raise UpdateError("merge", "cannot merge the configured custom-alpha source ref")
        tag = self.c.get("upstreamTagPrefix", "rust-v") + version
        p = _run(["git", "fetch", self.c.get("upstreamRemote", "upstream"), f"refs/tags/{tag}"], cwd=wt, check=False)
        if p.returncode:
            raise UpdateError("merge", f"cannot fetch official tag {tag}")
        target = _run(["git", "rev-parse", "FETCH_HEAD^{commit}"], cwd=wt).stdout.strip()
        p = _run(["git", "merge", "--no-edit", target], cwd=wt, check=False)
        if p.returncode:
            unmerged = _run(["git", "diff", "--name-only", "--diff-filter=U"], cwd=wt).stdout.splitlines()
            if unmerged != ["codex-rs/Cargo.toml"]:
                raise UpdateError("conflict", "merge has conflicts beyond codex-rs/Cargo.toml")
            cargo = wt / "codex-rs" / "Cargo.toml"
            resolve_version_conflict(cargo, version)
            _run(["git", "add", "codex-rs/Cargo.toml"], cwd=wt)
            _run(["git", "-c", "user.name=Custom Alpha Updater", "-c", "user.email=updater@localhost",
                  "commit", "--no-edit"], cwd=wt)
        return wt

    def _build(self, version):
        wt = self._merge_version(version)
        rs = wt / "codex-rs"
        env = os.environ.copy()
        env.update({"CARGO_TARGET_DIR": str(Path(self.c["cargoTargetDir"]).resolve()), "CARGO_BUILD_JOBS": "1"})
        temp = Path(env["CARGO_TARGET_DIR"]).parent / "temp"
        temp.mkdir(parents=True, exist_ok=True)
        env["TEMP"] = env["TMP"] = str(temp)
        p = _run(["just", "test", "-p", "codex-cli"], cwd=rs, env=env, check=False)
        if p.returncode:
            raise UpdateError("test", f"scoped just test failed: {p.stdout[-700:]}")
        p = _run(["cargo", "build", "-p", "codex-cli", "--bin", "codex"], cwd=rs, env=env, check=False)
        if p.returncode:
            raise UpdateError("build", f"codex build failed: {p.stdout[-700:]}")
        exe = Path(env["CARGO_TARGET_DIR"]) / "debug" / "codex.exe"
        if not exe.is_file():
            raise UpdateError("build", "cargo succeeded but codex.exe is missing")
        info = _run([str(exe), "--version"], cwd=rs, env=env)
        if version not in info.stdout:
            raise UpdateError("build", "built codex.exe version does not match npm alpha")
        return exe

    def _template(self, version):
        meta = json.loads(_https("https://registry.npmjs.org/%40openai%2Fcodex-win32-x64", 32 * 1024 * 1024))
        pkg = meta.get("versions", {}).get(version)
        if not pkg:
            raise UpdateError("package", "platform package does not have the selected alpha version")
        dist = pkg.get("dist", {})
        integrity = dist.get("integrity", "")
        match = re.search(r"(?:^| )sha512-([A-Za-z0-9+/=]+)(?: |$)", integrity)
        if not match or not dist.get("tarball"):
            raise UpdateError("package", "npm package has no verifiable sha512 integrity")
        template_root = Path(self.c["officialPackageTemplatePath"]).resolve()
        target = template_root / version
        if not target.exists():
            blob = _https(dist["tarball"], MAX_TARBALL)
            if base64.b64encode(hashlib.sha512(blob).digest()).decode() != match.group(1):
                raise UpdateError("package", "npm package integrity mismatch")
            template_root.mkdir(parents=True, exist_ok=True)
            temp = Path(tempfile.mkdtemp(prefix=".alpha-", dir=template_root))
            try:
                expanded = 0
                with tarfile.open(fileobj=__import__("io").BytesIO(blob), mode="r:gz") as archive:
                    members = archive.getmembers()
                    if len(members) > 50000:
                        raise UpdateError("package", "npm package has too many entries")
                    for m in members:
                        name = PurePosixPath(m.name)
                        if name.is_absolute() or ".." in name.parts or not name.parts or name.parts[0] != "package":
                            raise UpdateError("package", "unsafe npm package path")
                        if not (m.isdir() or m.isfile()) or m.issym() or m.islnk():
                            raise UpdateError("package", "unsupported npm archive entry")
                        expanded += m.size
                        if expanded > MAX_EXPANDED:
                            raise UpdateError("package", "expanded npm package exceeds limit")
                        archive.extract(m, temp)
                extracted = temp / "package"
                if not (extracted / "codex-package.json").is_file() or not (extracted / "bin" / "codex.exe").is_file():
                    raise UpdateError("package", "official package is missing codex-package.json or bin/codex.exe")
                info = json.loads((extracted / "package.json").read_text(encoding="utf-8"))
                if info.get("version") != version:
                    raise UpdateError("package", "official package version does not match selected alpha")
                os.replace(extracted, target)
            finally:
                shutil.rmtree(temp, ignore_errors=True)
        return target, integrity

    def build_candidate(self, version):
        exe = self._build(version)
        template, integrity = self._template(version)
        self.artifacts.mkdir(parents=True, exist_ok=True)
        candidates = self.artifacts / "candidates"
        candidates.mkdir(exist_ok=True)
        final = candidates / version
        if final.exists():
            manifest = json.loads((final / "candidate.json").read_text(encoding="utf-8"))
            if manifest.get("version") != version or _hash(final / "bin" / "codex.exe") != manifest.get("codexSha256"):
                raise UpdateError("package", "existing immutable candidate is corrupt")
            return final
        temp = Path(tempfile.mkdtemp(prefix=f".{version}-", dir=candidates))
        try:
            shutil.copytree(template, temp, dirs_exist_ok=True)
            target_exe = temp / "bin" / "codex.exe"
            shutil.copy2(exe, target_exe)
            info = _run([str(target_exe), "--version"], cwd=temp)
            if version not in info.stdout:
                raise UpdateError("package", "candidate codex.exe version mismatch")
            manifest = {"version": version, "npmIntegrity": integrity, "codexSha256": _hash(target_exe),
                        "helpers": {str(p.relative_to(temp).as_posix()): _hash(p) for p in temp.rglob("*")
                                    if p.is_file() and p != target_exe}}
            if not manifest["helpers"]:
                raise UpdateError("package", "official package has no helpers or resources")
            (temp / "candidate.json").write_text(json.dumps(manifest, indent=2), encoding="utf-8")
            os.replace(temp, final)
        except Exception:
            shutil.rmtree(temp, ignore_errors=True)
            raise
        prepared = {"version": version, "path": str(final.resolve())}
        self._atomic(self.artifacts / "prepared.json", prepared)
        return final

    @staticmethod
    def _atomic(path, value):
        path.parent.mkdir(parents=True, exist_ok=True)
        tmp = path.with_suffix(path.suffix + ".tmp")
        tmp.write_text(json.dumps(value, indent=2), encoding="utf-8")
        os.replace(tmp, path)

    def prepared(self):
        path = self.artifacts / "prepared.json"
        if not path.exists():
            return None
        record = json.loads(path.read_text(encoding="utf-8"))
        candidate = Path(record["path"]).resolve()
        candidates = (self.artifacts / "candidates").resolve()
        if candidate.parent != candidates or not (candidate / "candidate.json").is_file():
            raise UpdateError("package", "prepared candidate path is invalid")
        manifest = json.loads((candidate / "candidate.json").read_text(encoding="utf-8"))
        if record.get("version") != manifest.get("version") or _hash(candidate / "bin" / "codex.exe") != manifest.get("codexSha256"):
            raise UpdateError("package", "prepared candidate failed hash verification")
        return candidate

    def safe(self):
        homes = [str(Path(p).resolve()) for p in self.c["codexHomes"]]
        script = "Get-CimInstance Win32_Process -Filter \"name='codex.exe'\" | Select-Object ExecutablePath,CommandLine | ConvertTo-Json -Compress"
        p = _run(["powershell.exe", "-NoProfile", "-NonInteractive", "-Command", script], check=False)
        if p.returncode:
            return False
        try:
            processes = json.loads(p.stdout) if p.stdout.strip() else []
            if isinstance(processes, dict):
                processes = [processes]
            daemons = {str(Path(h) / "packages" / "app-server-daemon" / "current" / "bin" / "codex.exe").casefold() for h in homes}
            for proc in processes:
                image = str(proc.get("ExecutablePath", "")).casefold()
                if image in daemons:
                    continue
                return False
        except Exception:
            return False
        return all(_verify_idle(home) for home in homes)

    def activate(self, candidate):
        exe = candidate / "bin" / "codex.exe"
        for home in self.c["codexHomes"]:
            env = os.environ.copy()
            env["CODEX_HOME"] = str(Path(home).resolve())
            p = _run([str(exe), "app-server", "daemon", "update", "--from-cli", "--yes"], cwd=candidate, env=env, check=False)
            if p.returncode:
                raise UpdateError("activate", f"official daemon update failed for {Path(home).name}: {p.stdout[-500:]}")
        manifest = json.loads((candidate / "candidate.json").read_text(encoding="utf-8"))
        self._atomic(self.artifacts / "last-known-good.json", {"version": manifest["version"], "path": str(candidate)})
        (self.artifacts / "prepared.json").unlink(missing_ok=True)


def write_status(artifacts, state, stage, message):
    root = Path(artifacts)
    root.mkdir(parents=True, exist_ok=True)
    value = {"at": datetime.now(timezone.utc).isoformat(), "state": state, "stage": stage, "message": message[:500]}
    tmp = root / "status.json.tmp"
    tmp.write_text(json.dumps(value, indent=2), encoding="utf-8")
    os.replace(tmp, root / "status.json")
    log = root / "updater.log"
    old = log.read_bytes()[-(64 * 1024):] if log.exists() else b""
    line = f"{value['at']} {state} {stage} {value['message']}\n".encode("utf-8", "replace")
    log.write_bytes((old + line)[-64 * 1024:])


def run_pipeline(ops, artifacts):
    try:
        candidate = ops.prepared()
        if candidate is None:
            candidate = ops.build_candidate(ops.latest())
        if not ops.safe():
            write_status(artifacts, "deferred", "idle-gate", "running client, loaded thread, or idle state unknown")
            return 0
        ops.activate(candidate)
        write_status(artifacts, "activated", "complete", f"Codex alpha {candidate.name} pinned")
        return 0
    except UpdateError as exc:
        write_status(artifacts, "failed", exc.stage, str(exc))
        return 1
    except Exception as exc:
        write_status(artifacts, "failed", "unexpected", str(exc)[:500])
        return 1


def main(argv=None):
    argv = sys.argv[1:] if argv is None else argv
    if len(argv) != 1:
        print("usage: update.py CONFIG.json", file=sys.stderr)
        return 2
    config = json.loads(Path(argv[0]).read_text(encoding="utf-8"))
    ops = Ops(config)
    return run_pipeline(ops, config["artifactsRoot"])


if __name__ == "__main__":
    raise SystemExit(main())
