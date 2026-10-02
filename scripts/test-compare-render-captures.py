#!/usr/bin/env python3
"""Check that capture comparison detects changed pixels and mismatched scenes."""
import importlib.util
import json
from pathlib import Path
import struct
import tempfile
import unittest
import zlib

spec = importlib.util.spec_from_file_location("compare", Path(__file__).with_name("compare-render-captures.py"))
compare = importlib.util.module_from_spec(spec)
spec.loader.exec_module(compare)


def chunk(kind, data):
    return struct.pack(">I", len(data))+kind+data+struct.pack(">I", zlib.crc32(kind+data) & 0xffffffff)


def png(path, pixels, text=b"test"):
    header = struct.pack(">IIBBBBB", 2, 2, 8, 6, 0, 0, 0)
    raw = b"\0"+pixels[:8]+b"\0"+pixels[8:]
    path.write_bytes(b"\x89PNG\r\n\x1a\n"+chunk(b"IHDR",header)+chunk(b"tEXt",b"label\0"+text)+chunk(b"IDAT",zlib.compress(raw))+chunk(b"IEND",b""))


class CaptureComparisonTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.baseline = Path(self.temp.name)/"baseline"
        self.candidate = Path(self.temp.name)/"candidate"
        self.baseline.mkdir()
        self.candidate.mkdir()
        self.pixels = bytes((30,70,110,255, 100,150,200,255, 50,180,40,255, 250,120,30,255))
        self.manifest = {field: 0 for field in compare.STATE_FIELDS}
        self.manifest.update({"fixture": {"id":"scene","eye":[1,2,3],"yaw":0.5}, "output":[2,2], "internal":[2,2], "variant":"baseline"})
        for directory in (self.baseline,self.candidate):
            (directory/"scene.json").write_text(json.dumps(self.manifest))
            png(directory/"scene.png",self.pixels)

    def test_aa_compares_pixels_independently_of_png_metadata(self):
        png(self.candidate/"scene.png", self.pixels, b"different metadata")
        result = compare.compare_capture(self.baseline,self.candidate,"scene",strict_aa=True)
        self.assertEqual(result["errors"], [])
        self.assertTrue(result["difference"]["exactPixelMatch"])
        self.assertNotEqual(result["baselineFileSha256"], result["candidateFileSha256"])
        self.assertEqual(result["baseline"]["pixelSha256"], result["candidate"]["pixelSha256"])

    def test_one_changed_color_fails_aa_and_measures_exact_fraction(self):
        pixels = bytearray(self.pixels)
        pixels[2] += 12
        png(self.candidate/"scene.png",pixels)
        result = compare.compare_capture(self.baseline,self.candidate,"scene",strict_aa=True)
        self.assertIn("strict A/A pixels differ",result["errors"])
        self.assertEqual(result["difference"]["changedPixelPercent"],25)
        self.assertEqual(result["difference"]["meanAbsoluteRgb8"],1)

    def test_camera_mismatch_is_rejected_even_with_identical_pixels(self):
        self.manifest["fixture"]["eye"][0] += 1
        (self.candidate/"scene.json").write_text(json.dumps(self.manifest))
        result = compare.compare_capture(self.baseline,self.candidate,"scene")
        self.assertFalse(result["stateMatch"])
        self.assertIn("fixture",result["stateMismatches"])

    def test_black_or_transparent_capture_is_rejected(self):
        png(self.candidate/"scene.png",bytes((0,0,0,0))*4)
        errors = compare.compare_capture(self.baseline,self.candidate,"scene")["errors"]
        self.assertIn("candidate capture is nearly black",errors)
        self.assertIn("candidate capture contains nonopaque pixels",errors)

    def test_expected_effect_and_corrupt_png_cannot_pass_silently(self):
        result = compare.compare_capture(self.baseline,self.candidate,"scene",require_effect=True)
        self.assertIn("expected a visible effect but all pixels match",result["errors"])
        path = self.candidate/"scene.png"
        data = bytearray(path.read_bytes())
        data[-5] ^= 1
        path.write_bytes(data)
        with self.assertRaisesRegex(ValueError,"corrupt PNG"):
            compare.read_png(path)


if __name__ == "__main__":
    unittest.main()
