#!/usr/bin/env python3
import json
import socket
with socket.create_connection(("127.0.0.1", 1705), timeout=2) as client:
    client.sendall(b'{"id":1,"jsonrpc":"2.0","method":"Server.GetStatus"}\r\n')
    replies = client.makefile("rb")
    for _ in range(64):
        response = json.loads(replies.readline(1024 * 1024))
        if response.get("id") == 1:
            assert "server" in response["result"]
            break
    else:
        raise SystemExit("No Snapcast control response")
