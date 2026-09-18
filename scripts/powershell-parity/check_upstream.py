"""Report new Codex source releases and check patch portability without running them."""

import argparse
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import urllib.request


ROOT = Path(__file__).resolve().parents[2]
TAG_PATTERN = re.compile(r"rust-v[0-9][0-9A-Za-z.-]*\Z")


def git(repo, *args, **kwargs):
    return subprocess.run(
        ["git", "-C", str(repo), *args], check=True, capture_output=True, **kwargs
    )


def check_patch(repo, base, head, target):
    commits = [
        git(
            repo,
            "rev-parse",
            "--verify",
            "--end-of-options",
            f"{ref}^{{commit}}",
            text=True,
        ).stdout.strip()
        for ref in (base, head, target)
    ]
    patch = git(
        repo, "diff", "--binary", commits[0], commits[1], "--", "codex-rs"
    ).stdout
    if not patch:
        raise ValueError(
            "No backend patch found between the recorded baseline and HEAD"
        )
    with tempfile.TemporaryDirectory(prefix="codex-parity-index-") as directory:
        environment = os.environ.copy()
        environment["GIT_INDEX_FILE"] = str(Path(directory) / "index")
        git(repo, "read-tree", commits[2], env=environment)
        result = subprocess.run(
            [
                "git",
                "-C",
                str(repo),
                "apply",
                "--cached",
                "--check",
                "--whitespace=nowarn",
            ],
            input=patch,
            capture_output=True,
            env=environment,
        )
        if result.returncode == 0:
            return {
                "status": "applies",
                "details": "Patch applies; compilation and desktop acceptance still required.",
            }
        reverse = subprocess.run(
            [
                "git",
                "-C",
                str(repo),
                "apply",
                "--cached",
                "--check",
                "--reverse",
                "--whitespace=nowarn",
            ],
            input=patch,
            capture_output=True,
            env=environment,
        )
        if reverse.returncode == 0:
            return {
                "status": "already-applied",
                "details": "Exact patch is already present; verify behavior before retiring it.",
            }
        return {
            "status": "conflict",
            "details": result.stderr.decode("utf-8", errors="replace").strip(),
        }


def latest_tag():
    request = urllib.request.Request(
        "https://api.github.com/repos/openai/codex/releases?per_page=100",
        headers={
            "Accept": "application/vnd.github+json",
            "User-Agent": "codex-powershell-actions",
        },
    )
    token = os.environ.get("GH_TOKEN")
    if token:
        request.add_header("Authorization", f"Bearer {token}")
    with urllib.request.urlopen(request, timeout=30) as response:
        releases = json.load(response)
    candidates = [
        item
        for item in releases
        if not item["draft"] and TAG_PATTERN.fullmatch(item["tag_name"])
    ]
    if not candidates:
        raise ValueError("No published Rust release found")
    return max(candidates, key=lambda item: item["published_at"])["tag_name"]


def run(args):
    config = json.loads(
        (Path(__file__).with_name("compatibility.json")).read_text(encoding="utf-8")
    )
    tag = args.tag or latest_tag()
    if not TAG_PATTERN.fullmatch(tag):
        raise ValueError("Expected an official rust-v... release tag")
    git(
        ROOT,
        "fetch",
        "--no-tags",
        "https://github.com/openai/codex.git",
        f"refs/tags/{tag}",
    )
    target = git(ROOT, "rev-parse", "FETCH_HEAD^{commit}", text=True).stdout.strip()
    result = {
        "baselineTag": config["upstreamTag"],
        "candidateTag": tag,
        "candidateCommit": target,
        "newSourceVersion": target != config["upstreamCommit"],
        "desktopCompatibility": "unverified for candidate; source release is not a desktop version",
        **check_patch(ROOT, config["upstreamCommit"], "HEAD", target),
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    summary = (
        f"## PowerShell patch upstream check\n\n"
        f"- Baseline: `{result['baselineTag']}`\n"
        f"- Candidate: `{tag}` (`{target}`)\n"
        f"- New source version: **{result['newSourceVersion']}**\n"
        f"- Patch portability: **{result['status']}**\n\n"
        "This report does not update a branch, publish a release, or establish desktop compatibility.\n"
        "Port the patch on a new version branch, run Windows CI and app-server checks, then verify the actual desktop UI.\n"
    )
    print(summary)
    if os.environ.get("GITHUB_STEP_SUMMARY"):
        with open(os.environ["GITHUB_STEP_SUMMARY"], "a", encoding="utf-8") as stream:
            stream.write(summary)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--tag",
        help="Check this official source tag instead of the latest published Rust release",
    )
    parser.add_argument(
        "--output", type=Path, default=ROOT / ".local/upstream-check.json"
    )
    run(parser.parse_args())
