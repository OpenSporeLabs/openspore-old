#!/usr/bin/env python3
"""Drive bevy_brp_mcp over stdio, one tool call at a time.

A minimal MCP client: initialize, then call the named tools in order and print
each result. Exists so the smoke test exercises the *same* stdio path opencode
uses, rather than a private one.
"""
import json
import subprocess
import sys


def main() -> int:
    # argv: script_name call1 call2 ...   each call is a JSON object of args
    calls = json.loads(sys.argv[1])
    proc = subprocess.Popen(
        ["/home/juanr/.local/bin/bevy_brp_mcp"],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL,
        text=True,
        bufsize=1,
    )

    def send(msg):
        proc.stdin.write(json.dumps(msg) + "\n")
        proc.stdin.flush()

    def recv():
        while True:
            line = proc.stdout.readline()
            if not line:
                raise SystemExit("mcp server closed stdout")
            line = line.strip()
            if not line:
                continue
            return json.loads(line)

    send({
        "jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05", "capabilities": {},
            "clientInfo": {"name": "openspore-smoke", "version": "1"},
        },
    })
    while recv().get("id") != 1:
        pass
    send({"jsonrpc": "2.0", "method": "notifications/initialized"})

    ok = True
    for i, call in enumerate(calls, start=10):
        send({
            "jsonrpc": "2.0", "id": i, "method": "tools/call",
            "params": {"name": call["tool"], "arguments": call.get("args", {})},
        })
        while True:
            resp = recv()
            if resp.get("id") != i:
                continue
            break
        if "error" in resp:
            ok = False
            print(f"### {call['tool']} -> MCP ERROR: {resp['error']}")
            continue
        content = resp["result"].get("content", [])
        text = "\n".join(c.get("text", "") for c in content if c.get("type") == "text")
        flag = "" if not resp["result"].get("isError") else "  [isError]"
        print(f"### {call['tool']}{flag}")
        print(text[:400000])
        print()
    try:
        proc.stdin.close()
        proc.wait(timeout=10)
    except Exception:
        proc.kill()
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main())