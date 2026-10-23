#!/usr/bin/env python3
"""Own a separate, persistent manual Snapcast playground."""
import argparse
import os
from pathlib import Path
import shutil
import subprocess
ROOT = Path(__file__).resolve().parents[2]

def compose():
    if subprocess.run(["docker", "compose", "version"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL).returncode == 0:
        return ["docker", "compose"]
    if shutil.which("docker-compose"):
        return ["docker-compose"]
    raise SystemExit("Install Docker Compose v2.24.4 or newer (the smoke override uses !override)")

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=["up", "down", "reset", "logs", "tone", "stop-tone", "status"])
    args = parser.parse_args()
    command = compose() + ["-p", "sonicfast-snapcast-smoke", "-f", str(ROOT / "integration/snapcast/compose.yml"),
                           "-f", str(ROOT / "integration/snapcast/compose.smoke.yml")]
    # Keep random test-port environment overrides out of the manual project.
    env = dict(os.environ, SNAPCAST_CODEC=os.environ.get("SNAPCAST_CODEC", "flac"))
    commands = {"up": ["up", "--build", "-d", "--wait", "server", "reference-a", "reference-b"],
                "down": ["--profile", "tone", "down"], "reset": ["--profile", "tone", "down", "--volumes"],
                "logs": ["logs", "--tail", "100"], "status": ["ps"],
                "tone": ["--profile", "tone", "up", "--build", "-d", "tone"], "stop-tone": ["stop", "tone"]}
    subprocess.run(command + commands[args.action], cwd=ROOT, env=env, check=True)
    if args.action in ("up", "tone"):
        print("Snapcast smoke server: host 127.0.0.1, source port 4993, receiver port 1714")
        print("HTTP JSON-RPC: http://127.0.0.1:1781/jsonrpc; TCP control: 127.0.0.1:1715")
        print("Use Settings → Snapcast (also accessible before sign-in). See integration/snapcast/README.md.")
        if args.action == "up":
            print("Server ready. This command does not start an audio source.")
            print("To test receiving audio: make snapcast-smoke-tone")
        else:
            print("Test tone started. Enable Receive audio on port 1714 to listen.")
        print("Stop the tone before sending music from Sonicfast: make snapcast-smoke-stop-tone")

if __name__ == "__main__":
    main()
