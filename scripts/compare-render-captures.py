#!/usr/bin/env python3
"""Compare deterministic engine captures; image differences do not grade beauty.

Uses only Python's standard library. Match camera/world/render manifests before
comparing decoded pixels. --strict-aa gates baseline versus feature-off captures.
"""
import argparse
import hashlib
import json
import math
from pathlib import Path
import struct
import sys
import zlib

STATE_FIELDS = (
    "seed", "fixture", "output", "internal", "quality", "coverDensity",
    "antialiasing", "bloom", "weatherWarmupSeconds", "frozenAnimationSeconds",
    "warmupFrames", "people", "triangles", "meshBytes",
)


def paeth(a, b, c):
    p = a + b - c
    distances = (abs(p-a), abs(p-b), abs(p-c))
    return (a, b, c)[distances.index(min(distances))]


def read_png(path):
    data = path.read_bytes()
    if data[:8] != b"\x89PNG\r\n\x1a\n":
        raise ValueError(f"{path}: invalid PNG signature")
    pos, compressed, header = 8, bytearray(), None
    ended = False
    while pos < len(data):
        if pos + 12 > len(data):
            raise ValueError(f"{path}: truncated PNG chunk")
        size = struct.unpack_from(">I", data, pos)[0]
        kind = data[pos+4:pos+8]
        body = data[pos+8:pos+8+size]
        if len(body) != size or pos + size + 12 > len(data):
            raise ValueError(f"{path}: truncated PNG payload")
        crc = struct.unpack_from(">I", data, pos+8+size)[0]
        if zlib.crc32(kind+body) & 0xffffffff != crc:
            raise ValueError(f"{path}: corrupt PNG {kind!r} chunk")
        if kind == b"IHDR":
            header = struct.unpack(">IIBBBBB", body)
        elif kind == b"IDAT":
            compressed.extend(body)
        elif kind == b"IEND":
            ended = True
            break
        pos += size + 12
    if not header or not ended:
        raise ValueError(f"{path}: missing PNG header/end")
    width, height, bits, color, compression, filtering, interlace = header
    if bits != 8 or color not in (2, 6) or compression or filtering or interlace:
        raise ValueError(f"{path}: requires non-interlaced 8-bit RGB/RGBA PNG")
    if width < 1 or height < 1 or width*height > 32_000_000:
        raise ValueError(f"{path}: unsupported capture dimensions")
    channels = 4 if color == 6 else 3
    stride = width * channels
    raw = zlib.decompress(compressed)
    if len(raw) != height * (stride+1):
        raise ValueError(f"{path}: invalid decoded PNG size")
    rows, previous = bytearray(), bytearray(stride)
    for y in range(height):
        start = y * (stride+1)
        mode, row = raw[start], bytearray(raw[start+1:start+stride+1])
        if mode > 4:
            raise ValueError(f"{path}: invalid PNG filter {mode}")
        if mode:
            for x in range(stride):
                a = row[x-channels] if x >= channels else 0
                b = previous[x]
                c = previous[x-channels] if x >= channels else 0
                if mode == 1:
                    predictor = a
                elif mode == 2:
                    predictor = b
                elif mode == 3:
                    predictor = (a+b)//2
                else:
                    predictor = paeth(a,b,c)
                row[x] = (row[x] + predictor) & 255
        rows.extend(row)
        previous = row
    if channels == 3:
        rgba = bytearray(width*height*4)
        for src in range(0, len(rows), 3):
            dst = src//3*4
            rgba[dst:dst+4] = rows[src:src+3] + b"\xff"
        rows = rgba
    return width, height, bytes(rows), hashlib.sha256(data).hexdigest()


def image_metrics(pixels):
    count, dark, nonopaque, total = len(pixels)//4, 0, 0, 0
    histogram, colors = [0]*256, set()
    for i in range(0, len(pixels), 4):
        r, g, b, a = pixels[i:i+4]
        luma = (54*r + 183*g + 19*b) >> 8
        histogram[luma] += 1
        total += luma
        dark += max(r,g,b) <= 4
        nonopaque += a != 255
        colors.add((r << 16) | (g << 8) | b)
    def percentile(fraction):
        threshold, seen = math.ceil(count*fraction), 0
        for value, frequency in enumerate(histogram):
            seen += frequency
            if seen >= threshold:
                return value
    return {
        "pixelSha256": hashlib.sha256(pixels).hexdigest(),
        "meanLuma8": total/count,
        "lumaP05": percentile(0.05), "lumaP50": percentile(0.5),
        "lumaP95": percentile(0.95), "uniqueRgbColors": len(colors),
        "darkPixelPercent": dark/count*100,
        "nonOpaquePixelPercent": nonopaque/count*100,
        "nearlyBlack": dark/count > 0.999,
    }


def pixel_difference(a, b):
    absolute, squared, changed, maximum = 0, 0, 0, 0
    for i in range(0, len(a), 4):
        differences = [abs(a[i+j]-b[i+j]) for j in range(3)]
        absolute += sum(differences)
        squared += sum(d*d for d in differences)
        maximum = max(maximum, *differences)
        changed += a[i:i+4] != b[i:i+4]
    count = len(a)//4
    rms = math.sqrt(squared/(count*3))
    return {
        "exactPixelMatch": a == b,
        "changedPixelPercent": changed/count*100,
        "meanAbsoluteRgb8": absolute/(count*3),
        "rootMeanSquareRgb8": rms, "maxAbsoluteRgb8": maximum,
        "psnrDb": 20*math.log10(255/rms) if rms else None,
    }


def timing_difference(before, after):
    result = {}
    for field in ("nativeSubmission", "nativeCompleted", "gpuSpan"):
        a, b = before.get(field), after.get(field)
        if not a or not b:
            continue
        result[field] = {
            metric: {"baseline": a[metric], "candidate": b[metric],
                     "changePercent": (b[metric]/a[metric]-1)*100 if a[metric] else None}
            for metric in ("medianMs", "p95Ms", "p99Ms") if metric in a and metric in b
        }
    return result


def compare_capture(baseline, candidate, stem, strict_aa=False, require_effect=False):
    before = json.loads((baseline/f"{stem}.json").read_text())
    after = json.loads((candidate/f"{stem}.json").read_text())
    mismatches = {field: {"baseline": before.get(field), "candidate": after.get(field)}
                  for field in STATE_FIELDS
                  if field not in before or field not in after or before[field] != after[field]}
    # Use a geometry hash when the capture harness exports one. Counts alone
    # establish matching invariants, not identical vertex/index bytes.
    for field in ("geometrySha256", "geometryFingerprint"):
        if field in before or field in after:
            if before.get(field) != after.get(field):
                mismatches[field] = {"baseline": before.get(field), "candidate": after.get(field)}
    bw, bh, bp, bsha = read_png(baseline/f"{stem}.png")
    cw, ch, cp, csha = read_png(candidate/f"{stem}.png")
    errors = []
    if mismatches:
        errors.append("capture manifests differ")
    if (bw,bh) != (cw,ch):
        errors.append("PNG dimensions differ")
    if before.get("output") != [bw,bh] or after.get("output") != [cw,ch]:
        errors.append("PNG dimensions disagree with manifest")
    baseline_metrics, candidate_metrics = image_metrics(bp), image_metrics(cp)
    difference = pixel_difference(bp,cp) if (bw,bh) == (cw,ch) else None
    if candidate_metrics["nearlyBlack"]:
        errors.append("candidate capture is nearly black")
    if candidate_metrics["nonOpaquePixelPercent"]:
        errors.append("candidate capture contains nonopaque pixels")
    if strict_aa and difference and not difference["exactPixelMatch"]:
        errors.append("strict A/A pixels differ")
    if require_effect and difference and difference["exactPixelMatch"]:
        errors.append("expected a visible effect but all pixels match")
    return {
        "scene": stem, "baselineVariant": before.get("variant"),
        "candidateVariant": after.get("variant"), "stateMatch": not mismatches,
        "stateMismatches": mismatches, "pngDimensions": [bw,bh],
        "baselineFileSha256": bsha, "candidateFileSha256": csha,
        "baseline": baseline_metrics, "candidate": candidate_metrics,
        "difference": difference, "timing": timing_difference(before,after),
        "errors": errors,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("baseline", type=Path)
    parser.add_argument("candidate", type=Path)
    parser.add_argument("--scene", action="append", help="Scene filename stem; repeat as needed")
    parser.add_argument("--strict-aa", action="store_true", help="Require every decoded pixel to match")
    parser.add_argument("--require-effect", action="store_true", help="Reject scenes with no pixel changes")
    parser.add_argument("--out", type=Path)
    args = parser.parse_args()
    scenes = args.scene or sorted(path.stem for path in args.baseline.glob("*.png"))
    if not scenes:
        parser.error("baseline directory contains no PNG captures")
    captures = []
    for stem in scenes:
        if Path(stem).name != stem:
            parser.error("scene must be a filename stem")
        try:
            captures.append(compare_capture(args.baseline,args.candidate,stem,args.strict_aa,args.require_effect))
        except (OSError, ValueError, struct.error, zlib.error) as error:
            captures.append({"scene": stem, "errors": [str(error)]})
    report = {"passed": all(not capture["errors"] for capture in captures),
              "strictAA": args.strict_aa,
              "note": "Pixel differences establish identity/change, not visual quality. Native timings are not browser FPS. Geometry counts do not hash mesh contents.",
              "captures": captures}
    encoded = json.dumps(report, indent=2, allow_nan=False)
    if args.out:
        args.out.parent.mkdir(parents=True, exist_ok=True)
        args.out.write_text(encoded+"\n")
    print(encoded)
    return 0 if report["passed"] else 2


if __name__ == "__main__":
    sys.exit(main())
