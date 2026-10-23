"""Servidor Git HTTP mínimo com Basic Auth, só para o spike F00.26 testar credenciais (não é produto)."""
import base64, os, subprocess, sys
from http.server import BaseHTTPRequestHandler, HTTPServer

ROOT, USER, PWD, PORT = sys.argv[1], sys.argv[2], sys.argv[3], int(sys.argv[4])
EXPECT = "Basic " + base64.b64encode(f"{USER}:{PWD}".encode()).decode()

class H(BaseHTTPRequestHandler):
    def _go(self):
        if self.headers.get("Authorization") != EXPECT:
            self.send_response(401); self.send_header("WWW-Authenticate", 'Basic realm="git"'); self.send_header("Content-Length", "0"); self.end_headers(); return
        path, _, qs = self.path.partition("?")
        if self.headers.get("Transfer-Encoding", "").lower() == "chunked":
            body = b""
            while True:
                size = int(self.rfile.readline().strip() or b"0", 16)
                if size == 0:
                    self.rfile.readline(); break
                body += self.rfile.read(size); self.rfile.readline()
        else:
            n = int(self.headers.get("Content-Length") or 0)
            body = self.rfile.read(n) if n else b""
        env = {"GIT_PROJECT_ROOT": ROOT, "GIT_HTTP_EXPORT_ALL": "1", "PATH_INFO": path, "QUERY_STRING": qs,
               "REQUEST_METHOD": self.command, "CONTENT_TYPE": self.headers.get("Content-Type", ""), "CONTENT_LENGTH": str(len(body)),
               "REMOTE_USER": USER, "PATH": os.environ["PATH"], "GIT_CONFIG_COUNT": "1", "GIT_CONFIG_KEY_0": "http.receivepack", "GIT_CONFIG_VALUE_0": "true"}
        if self.headers.get("Content-Encoding"): env["HTTP_CONTENT_ENCODING"] = self.headers["Content-Encoding"]
        out = subprocess.run(["git", "http-backend"], input=body, env=env, capture_output=True).stdout
        head, _, payload = out.partition(b"\r\n\r\n")
        status, headers = 200, []
        for line in head.split(b"\r\n"):
            k, _, v = line.decode().partition(": ")
            if k.lower() == "status": status = int(v.split()[0])
            elif k: headers.append((k, v))
        self.send_response(status)
        for k, v in headers: self.send_header(k, v)
        self.send_header("Content-Length", str(len(payload))); self.end_headers(); self.wfile.write(payload)
    do_GET = do_POST = _go
    def log_message(self, *a): pass

HTTPServer(("127.0.0.1", PORT), H).serve_forever()
