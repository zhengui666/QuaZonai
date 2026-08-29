"""Minimal remote Candidate verification stub used only by browser E2E CI."""

from __future__ import annotations

from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json


class Handler(BaseHTTPRequestHandler):
    def do_POST(self) -> None:  # noqa: N802
        if self.path != "/v1/candidates/verify":
            self.send_error(404)
            return
        try:
            length = int(self.headers.get("content-length", "0"))
            payload = json.loads(self.rfile.read(length))
            candidate_id = payload["candidate_id"]
        except (KeyError, TypeError, ValueError, json.JSONDecodeError):
            self.send_error(400)
            return
        body = json.dumps(
            {
                "protocol_version": "1",
                "runtime_version": "1.231.0",
                "candidate_id": candidate_id,
                "compatible": True,
                "findings": [],
            }
        ).encode("utf-8")
        self.send_response(200)
        self.send_header("content-type", "application/json")
        self.send_header("content-length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, format: str, *args: object) -> None:
        del format, args


if __name__ == "__main__":
    ThreadingHTTPServer(("127.0.0.1", 18081), Handler).serve_forever()
