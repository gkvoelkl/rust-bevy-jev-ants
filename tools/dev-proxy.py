#!/usr/bin/env python3
"""Forwards /v1/... to the TypeSafe API without the browser's Origin header.

This exists because of one measurement (2026-09-24): the API looks at `Origin`
on the request itself, not only on the CORS preflight, and rejects one it does
not know with `400 Disallowed CORS origin`. A browser attaches `Origin` to every
POST, including a same-origin one — so `http://localhost:8080` travelled through
Trunk's proxy untouched, straight into that rejection, and every ant's request
came back a failure while the same call from a terminal worked.

Trunk's built-in proxy forwards headers verbatim and cannot drop one, so this
sits between the two. In a deployment nothing here is needed: the web server
that already forwards /api does the same one thing, for example nginx with
`proxy_set_header Origin "";`.

    python3 tools/dev-proxy.py        # 127.0.0.1:8081 -> https://api.typesafe.ai

It handles no keys of its own. The player's Authorization header is passed
through as it arrives and is never written down — not even to this script's log,
which prints the method, the path and the status and nothing else.
"""

import sys
import urllib.error
import urllib.request
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

LISTEN = ("127.0.0.1", 8081)
UPSTREAM = "https://api.typesafe.ai"

# Everything else is passed on. `Origin` and `Referer` are the two that say
# "a page sent me", and saying that is exactly what gets the request refused.
STRIPPED = {"origin", "referer", "host", "connection", "content-length"}


class Proxy(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def do_POST(self):
        length = int(self.headers.get("Content-Length", 0))
        body = self.rfile.read(length) if length else b""

        headers = {
            name: value
            for name, value in self.headers.items()
            if name.lower() not in STRIPPED
        }

        request = urllib.request.Request(
            UPSTREAM + self.path, data=body, headers=headers, method="POST"
        )

        try:
            with urllib.request.urlopen(request, timeout=30) as answer:
                self.relay(answer.status, answer.headers.get_content_type(), answer.read())
        except urllib.error.HTTPError as refused:
            # The API's own answer, passed on whole: the game shows the message
            # in the inspector, and a swallowed 401 would look like a dead line.
            self.relay(refused.code, refused.headers.get_content_type(), refused.read())
        except Exception as broken:  # noqa: BLE001 - a dev proxy must not die
            self.relay(502, "application/json", str(broken).encode())

    def relay(self, status, content_type, body):
        self.send_response(status)
        self.send_header("Content-Type", content_type or "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, fmt, *args):
        sys.stderr.write("dev-proxy %s\n" % (fmt % args))


if __name__ == "__main__":
    print(f"dev-proxy: http://{LISTEN[0]}:{LISTEN[1]} -> {UPSTREAM} (without Origin)")
    ThreadingHTTPServer(LISTEN, Proxy).serve_forever()
