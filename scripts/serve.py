#!/usr/bin/env python3
"""Local static server with explicit WASM MIME and disabled stale development cache."""
import http.server
from pathlib import Path
import os
os.chdir(Path(__file__).resolve().parent.parent / 'dist')
class Handler(http.server.SimpleHTTPRequestHandler):
    extensions_map = {**http.server.SimpleHTTPRequestHandler.extensions_map, '.wasm': 'application/wasm', '.js': 'text/javascript'}
    def end_headers(self):
        self.send_header('Cache-Control', 'no-cache')
        super().end_headers()
server = http.server.ThreadingHTTPServer(('127.0.0.1', 4173), Handler)
print('FantasyLand: http://127.0.0.1:4173', flush=True)
server.serve_forever()
