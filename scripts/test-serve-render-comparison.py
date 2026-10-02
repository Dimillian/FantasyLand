#!/usr/bin/env python3
"""Exercise report POST persistence without starting or restarting a server."""
import importlib.util
import io
import json
from pathlib import Path
import tempfile
import unittest

spec=importlib.util.spec_from_file_location('render_server',Path(__file__).with_name('serve-render-comparison.py'))
server=importlib.util.module_from_spec(spec)
spec.loader.exec_module(server)


class ReportPersistence(unittest.TestCase):
    def setUp(self):
        self.directory=tempfile.TemporaryDirectory()
        self.original_artifacts=server.artifacts
        server.artifacts=Path(self.directory.name)

    def tearDown(self):
        server.artifacts=self.original_artifacts
        self.directory.cleanup()

    def post(self,path,report):
        body=json.dumps(report).encode()
        handler=server.Handler.__new__(server.Handler)
        handler.path=path
        handler.headers={'Content-Length':str(len(body))}
        handler.rfile=io.BytesIO(body)
        handler.wfile=io.BytesIO()
        statuses=[]
        handler.send_error=lambda status:statuses.append(status)
        handler.send_response=lambda status:statuses.append(status)
        handler.end_headers=lambda:None
        handler.do_POST()
        return statuses,handler.wfile.getvalue()

    def test_named_reports_update_latest_and_preserve_history(self):
        first={'options':{'stage':'off'},'gate':'A/A control recorded.'}
        second={'options':{'stage':'full'},'gate':'Performance comparison provisional.'}
        for name,report in [('browser-control.json',first),('browser-paired.json',second)]:
            status,body=self.post('/comparison/report?name='+name,report)
            self.assertEqual(status,[200])
            self.assertEqual(json.loads(body),{'saved':True})
            self.assertEqual((server.artifacts/name).read_bytes(),(server.artifacts/'browser-report.json').read_bytes())
        self.assertEqual(json.loads((server.artifacts/'browser-control.json').read_text()),first)
        self.assertEqual(json.loads((server.artifacts/'browser-report.json').read_text()),second)
        self.assertEqual(sorted(p.name for p in server.artifacts.iterdir()),['browser-control.json','browser-paired.json','browser-report.json'])

    def test_unnamed_report_uses_latest_alias(self):
        report={'summary':[{'role':'A'},{'role':'B'}]}
        self.assertEqual(self.post('/comparison/report',report)[0],[200])
        self.assertEqual(json.loads((server.artifacts/'browser-report.json').read_text()),report)
        self.assertEqual(len(list(server.artifacts.iterdir())),1)

    def test_rejected_name_does_not_overwrite_latest(self):
        self.post('/comparison/report',{'gate':'Existing report'})
        original=(server.artifacts/'browser-report.json').read_bytes()
        self.assertEqual(self.post('/comparison/report?name=browser-../escape.json',{'gate':'Invalid'})[0],[400])
        self.assertEqual((server.artifacts/'browser-report.json').read_bytes(),original)
        self.assertEqual(len(list(server.artifacts.iterdir())),1)


if __name__=='__main__':unittest.main()
