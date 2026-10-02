#!/usr/bin/env python3
"""Local-only rendering comparison server. Never publishes."""
import argparse,json,http.server,functools,tempfile
from pathlib import Path
from urllib.parse import urlparse,unquote,parse_qs
root=Path(__file__).resolve().parent.parent;artifacts=root/'output/rendering-upgrade'

def write_report(path,payload):
    """Publish complete JSON so a reload cannot read a partially written report."""
    temporary=None
    try:
        with tempfile.NamedTemporaryFile(mode='w',encoding='utf-8',dir=path.parent,delete=False) as output:
            temporary=Path(output.name);output.write(payload)
        temporary.replace(path)
    finally:
        if temporary is not None:temporary.unlink(missing_ok=True)

class Handler(http.server.SimpleHTTPRequestHandler):
    extensions_map={**http.server.SimpleHTTPRequestHandler.extensions_map,'.wasm':'application/wasm','.js':'text/javascript','.mjs':'text/javascript'}
    def translate_path(self,path):
        clean=unquote(urlparse(path).path)
        if clean.startswith('/comparison/'):
            target=(artifacts/clean[len('/comparison/'):]).resolve()
            if artifacts.resolve() in target.parents or target==artifacts.resolve():return str(target)
            return str(artifacts/'missing')
        return super().translate_path(path)
    def end_headers(self):self.send_header('Cache-Control','no-store');super().end_headers()
    def do_POST(self):
        if urlparse(self.path).path!='/comparison/report':self.send_error(404);return
        length=int(self.headers.get('Content-Length','0'))
        if length>8*1024*1024:self.send_error(413);return
        try:
            report=json.loads(self.rfile.read(length));name=parse_qs(urlparse(self.path).query).get('name',['browser-report.json'])[0];
            if not name.startswith('browser-') or not name.endswith('.json') or '/' in name or '..' in name:raise ValueError('Invalid report name')
            path=artifacts/name;path.parent.mkdir(parents=True,exist_ok=True);payload=json.dumps(report,indent=2)
            write_report(path,payload)
            if name!='browser-report.json':write_report(artifacts/'browser-report.json',payload)
        except (ValueError,OSError):self.send_error(400);return
        self.send_response(200);self.end_headers();self.wfile.write(b'{"saved":true}')
def main():
    parser=argparse.ArgumentParser();parser.add_argument('--port',type=int,default=4174);args=parser.parse_args()
    server=http.server.ThreadingHTTPServer(('127.0.0.1',args.port),functools.partial(Handler,directory=str(root/'dist')))
    print(f'Rendering comparison: http://127.0.0.1:{args.port}/rendering.html',flush=True);server.serve_forever()

if __name__=='__main__':main()
