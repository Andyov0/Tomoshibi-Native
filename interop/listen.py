# Answers every request with 401 and writes its method and path to stdout.
import http.server, sys
class H(http.server.BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"
    def do_GET(self):
        print("REQ", self.command, self.path.split("?")[0], "auth" if self.headers.get("Authorization") else "-", flush=True)
        self.send_response(401); self.send_header("Content-Length", "0"); self.end_headers()
    def log_message(self, *a): pass
http.server.ThreadingHTTPServer(("127.0.0.1", int(sys.argv[1])), H).serve_forever()
