"""Calls drui's REST API with DRUI_API_URL and DRUI_API_TOKEN when set (agent mode sets them, so two drui windows never mix up), else the URL and token from %TEMP%\\drui-api.json.

    python tools/dr.py GET /outline
    python tools/dr.py POST /claude_edit '{"id": 12}'      (a body of @file.json is read from that file, @- from stdin)
    python tools/dr.py GET "/export/12?format=png" out.png     (a binary reply is saved to the file)
    python tools/dr.py WRITE "/some/route" out_dir              (writes the "files" and "assets" of a reply into a folder)
"""
import base64
import json
import os
import re
import sys
import tempfile
import urllib.error
import urllib.request


def fix_path(p):
    """Git Bash rewrites an argument like /outline into C:/Program Files/Git/outline before python sees it: undo that, and accept a
    path without its leading slash"""
    m = re.match(r"^[A-Za-z]:[\\/].*?[\\/]Git(?:[\\/]usr)?([\\/].*)$", p)
    if m:
        p = m.group(1).replace("\\", "/")
    return p if p.startswith("/") else "/" + p


def main():
    if len(sys.argv) < 3:
        sys.exit(__doc__)
    method, path = sys.argv[1].upper(), fix_path(sys.argv[2])
    write_dir = None
    if method == "WRITE":
        method, write_dir = "GET", sys.argv[3]
        sys.argv = sys.argv[:3]
    body = sys.argv[3] if len(sys.argv) > 3 and not sys.argv[3].endswith((".png", ".pdf", ".svg")) else None
    out = sys.argv[-1] if len(sys.argv) > 3 and sys.argv[-1].endswith((".png", ".pdf", ".svg")) else None
    # a body of @file.json is read from the file ("-" from stdin): shells differ in how they pass quotes
    if body and body.startswith("@"):
        body = sys.stdin.read() if body == "@-" else open(body[1:], encoding="utf8").read()
    url, token = os.environ.get("DRUI_API_URL"), os.environ.get("DRUI_API_TOKEN")
    if not (url and token):
        info = json.load(open(os.path.join(tempfile.gettempdir(), "drui-api.json")))
        url, token = info["url"], info["token"]
    req = urllib.request.Request(url + path, data=body.encode() if body else None, method=method)
    req.add_header("Authorization", "Bearer " + token)
    try:
        with urllib.request.urlopen(req) as r:
            data, ctype = r.read(), r.headers.get("Content-Type", "")
    except urllib.error.HTTPError as e:
        sys.exit(f"HTTP {e.code}: {e.read().decode(errors='replace')}")
    if write_dir:
        reply = json.loads(data)
        n = 0
        for rel, text in reply.get("files", {}).items():
            dest = os.path.join(write_dir, rel)
            os.makedirs(os.path.dirname(dest), exist_ok=True)
            open(dest, "w", encoding="utf8").write(text)
            n += 1
        for rel, b64 in reply.get("assets", {}).items():
            dest = os.path.join(write_dir, rel)
            os.makedirs(os.path.dirname(dest), exist_ok=True)
            open(dest, "wb").write(base64.b64decode(b64))
            n += 1
        print(f"wrote {n} files to {write_dir}")
    elif out:
        open(out, "wb").write(data)
        print(f"saved {len(data)} bytes to {out}")
    elif "json" in ctype:
        print(json.dumps(json.loads(data), indent=1))
    else:
        sys.stdout.buffer.write(data)


main()
