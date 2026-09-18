"""Exercise real PowerShell execution and commandActions over app-server JSON-RPC.

Only the model is mocked. All commands read a dedicated fixture directory.
No user configuration, credentials, model service or project files are used.
"""

import argparse
import json
import os
from pathlib import Path
import queue
import subprocess
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer


CASES = [
    ("Get-Content -LiteralPath 'README.md' -TotalCount 2", ["read"], "TODO first"),
    ("Get-Content -LiteralPath 'README.md' -Encoding utf8", ["read"], "last line"),
    ("Get-Content -Raw -LiteralPath 'README.md'", ["read"], "last line"),
    ("rg -n 'TODO' '.'", ["search"], "TODO first"),
    ("rg --files '.'", ["listFiles"], "README.md"),
    ("Get-ChildItem -LiteralPath '.'", ["listFiles"], "README.md"),
    (
        "Select-String -LiteralPath 'README.md' -Pattern 'TODO'",
        ["search"],
        "TODO first",
    ),
    ("Get-Content README.md | Select-Object -Skip 1 -First 1", ["read"], "middle line"),
    (
        "gc README.md; rg TODO README.md; gci .",
        ["read", "search", "listFiles"],
        "TODO first",
    ),
    (
        "Get-Content -LiteralPath 'folder with spaces/README.md' -Raw",
        ["read"],
        "spaced fixture",
    ),
    (
        "Get-Content -LiteralPath 'skills/demo/SKILL.md' -TotalCount 20",
        ["read"],
        "parity-fixture",
    ),
]


def response_events(index, command=None):
    identifier = f"response-{index}"
    events = [{"type": "response.created", "response": {"id": identifier}}]
    if command is not None:
        events.append(
            {
                "type": "response.output_item.done",
                "item": {
                    "type": "function_call",
                    "call_id": f"parity-{index}",
                    "name": "exec_command",
                    "arguments": json.dumps(
                        {
                            "cmd": command,
                            "yield_time_ms": 10000,
                            "max_output_tokens": 2000,
                        }
                    ),
                },
            }
        )
    else:
        events.append(
            {
                "type": "response.output_item.done",
                "item": {
                    "type": "message",
                    "id": "parity-final",
                    "role": "assistant",
                    "content": [
                        {"type": "output_text", "text": "Parity fixtures completed."}
                    ],
                },
            }
        )
    events.append(
        {
            "type": "response.completed",
            "response": {
                "id": identifier,
                "usage": {"input_tokens": 0, "output_tokens": 0, "total_tokens": 0},
            },
        }
    )
    return "".join(f"data: {json.dumps(event)}\n\n" for event in events).encode()


def run(args):
    root = args.output.resolve()
    root.mkdir(parents=True, exist_ok=True)
    fixture = root / "fixture"
    fixture.mkdir(exist_ok=True)
    (fixture / "README.md").write_text(
        "TODO first\nmiddle line\nlast line\n", encoding="utf-8"
    )
    (fixture / "folder with spaces").mkdir(exist_ok=True)
    (fixture / "folder with spaces" / "README.md").write_text(
        "spaced fixture\n", encoding="utf-8"
    )
    (fixture / "skills" / "demo").mkdir(parents=True, exist_ok=True)
    (fixture / "skills" / "demo" / "SKILL.md").write_text(
        "---\nname: parity-fixture\ndescription: Local test fixture\n---\n",
        encoding="utf-8",
    )
    home = root / "home"
    home.mkdir(exist_ok=True)
    request_tools = set()
    request_index = 0

    class Model(BaseHTTPRequestHandler):
        def log_message(self, *unused):
            pass

        def do_POST(self):
            nonlocal request_index
            payload = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
            for tool in payload.get("tools", []):
                if "name" in tool:
                    request_tools.add(tool["name"])
            index = request_index
            request_index += 1
            data = response_events(
                index, CASES[index][0] if index < len(CASES) else None
            )
            self.send_response(200)
            self.send_header("Content-Type", "text/event-stream")
            self.send_header("Content-Length", str(len(data)))
            self.end_headers()
            self.wfile.write(data)

    server = ThreadingHTTPServer(("127.0.0.1", 0), Model)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    (home / "config.toml").write_text(
        'model = "gpt-5.1-codex"\nmodel_provider = "fixture"\n'
        'approval_policy = "never"\nsandbox_mode = "danger-full-access"\n'
        "[features]\nunified_exec = true\ncode_mode = false\n"
        '[model_providers.fixture]\nname = "Local fixture model"\nwire_api = "responses"\n'
        f'base_url = "http://127.0.0.1:{server.server_port}/v1"\n'
        "requires_openai_auth = false\nsupports_websockets = false\n",
        encoding="utf-8",
    )
    environment = os.environ.copy()
    environment["CODEX_HOME"] = str(home)
    environment.pop("OPENAI_API_KEY", None)
    environment.pop("CODEX_API_KEY", None)
    messages = queue.Queue()
    notifications = []
    stderr = (root / "stderr.log").open("w", encoding="utf-8")
    process = subprocess.Popen(
        [str(args.binary.resolve()), "app-server"],
        cwd=fixture,
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=stderr,
        text=True,
        encoding="utf-8",
        env=environment,
    )

    def read_messages():
        for line in process.stdout:
            try:
                messages.put(json.loads(line))
            except json.JSONDecodeError:
                messages.put({"invalid_stdout": line})
        messages.put({"process_ended": process.poll()})

    threading.Thread(target=read_messages, daemon=True).start()

    def send(method, params, identifier=None):
        message = {"method": method, "params": params}
        if identifier is not None:
            message["id"] = identifier
        process.stdin.write(json.dumps(message) + "\n")
        process.stdin.flush()

    def wait_for(predicate, seconds=90):
        deadline = time.monotonic() + seconds
        while time.monotonic() < deadline:
            message = messages.get(timeout=max(0.1, deadline - time.monotonic()))
            notifications.append(message)
            if "process_ended" in message:
                raise RuntimeError(f"app-server exited: {message}")
            if predicate(message):
                if "error" in message:
                    raise RuntimeError(message["error"])
                return message
            if "method" in message and "id" in message:
                raise RuntimeError(f"Unexpected server request: {message['method']}")
        raise TimeoutError("No matching app-server message")

    try:
        send(
            "initialize",
            {
                "clientInfo": {"name": "powershell_parity", "version": "1"},
                "capabilities": {"experimentalApi": True},
            },
            1,
        )
        initialized = wait_for(lambda message: message.get("id") == 1)
        send("initialized", {})
        send(
            "thread/start",
            {"cwd": str(fixture), "model": "gpt-5.1-codex", "modelProvider": "fixture"},
            2,
        )
        started = wait_for(lambda message: message.get("id") == 2)
        thread_id = started["result"]["thread"]["id"]
        send(
            "turn/start",
            {
                "threadId": thread_id,
                "input": [
                    {
                        "type": "text",
                        "text": "Run the local read-only parity fixtures.",
                        "text_elements": [],
                    }
                ],
            },
            3,
        )
        wait_for(lambda message: message.get("method") == "turn/completed", seconds=240)
        items = {}
        for message in notifications:
            if message.get("method") in ("item/started", "item/completed"):
                item = message.get("params", {}).get("item", {})
                if item.get("type") == "commandExecution":
                    items[(message["method"], item["id"])] = item
        results = []
        for index, (command, categories, output) in enumerate(CASES):
            for method in ("item/started", "item/completed"):
                item = items[(method, f"parity-{index}")]
                actions = item["commandActions"]
                assert [action["type"] for action in actions] == categories, (
                    command,
                    item,
                )
                assert all(action["command"] == command for action in actions), item
                for action in actions:
                    if action["type"] == "read":
                        assert Path(action["path"]).is_file(), action
                        assert Path(action["path"]).is_relative_to(fixture), action
                if method == "item/completed":
                    assert item["exitCode"] == 0, item
                    assert output in item["aggregatedOutput"], item
            results.append(
                {"command": command, "commandActions": actions, "passed": True}
            )
        send("thread/read", {"threadId": thread_id, "includeTurns": True}, 4)
        history = wait_for(lambda message: message.get("id") == 4)
        history_items = [
            item
            for turn in history["result"]["thread"]["turns"]
            for item in turn["items"]
            if item["type"] == "commandExecution"
        ]
        assert len(history_items) == len(CASES), history_items
        for item in history_items:
            assert (
                item["commandActions"]
                == items[("item/completed", item["id"])]["commandActions"]
            )
        report = {
            "binary": str(args.binary.resolve()),
            "initialized": initialized["result"],
            "cases": results,
            "historyVerified": True,
            "toolNames": sorted(request_tools),
        }
        (root / "result.json").write_text(
            json.dumps(report, indent=2), encoding="utf-8"
        )
        print(
            json.dumps(
                {
                    "passed": len(results),
                    "historyVerified": True,
                    "report": str(root / "result.json"),
                }
            )
        )
    finally:
        (root / "notifications.json").write_text(
            json.dumps(notifications, indent=2), encoding="utf-8"
        )
        process.stdin.close()
        try:
            process.wait(timeout=10)
        except subprocess.TimeoutExpired:
            process.terminate()
            process.wait(timeout=10)
        stderr.close()
        server.shutdown()


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    run(parser.parse_args())
