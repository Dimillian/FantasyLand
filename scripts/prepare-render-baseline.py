#!/usr/bin/env python3
"""Prepare the original renderer plus measurement hooks; never build or capture.

The archive retains the selected Git commit's shaders, renderer and dependencies.
Only lib.rs/verify.rs instrumentation and the shared native fixture harness are
added. Rebuilding generated dist/pkg files afterwards is expected. --force keeps
the previous source directory as a backup and never deletes sibling captures.
"""

import argparse
from datetime import datetime, timezone
import hashlib
import io
import json
import os
from pathlib import Path, PurePosixPath
import re
import shlex
import shutil
import subprocess
import sys
import tarfile
import tempfile


DEFAULT_REF = "76f77ea"
MARKER = ".render-baseline.json"
# The first local control used compact drain functions and a compact fixture
# harness before lastGpuPasses was added to its report. Exact old/new hash pairs
# permit adopting that source without rewriting it. Both sides must match so a
# later fixture change cannot silently reuse an older replay.
LEGACY_COMPATIBLE = {
    "src/lib.rs": (
        "dcc2aec1b9564242d8eeafc3648ceae11b835013fab7a86fa6b1d70312d73cc0",
        "8c4d8c70acd1d9cfac7fd5a023399049efacb51ca7ba44dd3a17f220d3824fbd"),
    "src/render_checks.rs": (
        "1dd893e6e7b317630180ef24703cec79f05d7b2c31baeae0473494127698477e",
        "a120fb8848d3faebb1a7db7dd32bd794588b7ea3f9839e3f55fd0353dd6cc071"),
}
GPU_DRAIN_METHODS = """    /// An asynchronous queue barrier for repeatable benchmark setup. Rendering
    /// remains nonblocking; the browser waits on RAF while the callback fires.
    pub fn begin_gpu_drain(&mut self) {
        self.gpu_drain
            .store(false, std::sync::atomic::Ordering::Release);
        let ready = self.gpu_drain.clone();
        self.renderer.queue.on_submitted_work_done(move || {
            ready.store(true, std::sync::atomic::Ordering::Release)
        });
    }
    pub fn gpu_drained(&self) -> bool {
        self.gpu_drain.load(std::sync::atomic::Ordering::Acquire)
    }
"""


def replace_once(text, before, after, name):
    if text.count(before) != 1:
        raise ValueError(f"{name}: expected one instrumentation anchor; found {text.count(before)}")
    return text.replace(before, after, 1)


def instrument_lib(text):
    text = replace_once(text, "    renderer: Renderer,\n", "    renderer: Renderer,\n"
                        "    gpu_drain: std::sync::Arc<std::sync::atomic::AtomicBool>,\n", "Game")
    text = replace_once(text, "    altitude: f32,\n", "    altitude: f32,\n"
                        "    camera_position: [f32; 3],\n", "GameState")
    text = replace_once(text, "            renderer,\n", "            renderer,\n"
                        "            gpu_drain: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true)),\n",
                        "Game constructor")
    text = replace_once(text, "            altitude: p.position.y,\n",
                        "            altitude: p.position.y,\n"
                        "            camera_position: p.eye().to_array(),\n", "cameraPosition")
    return replace_once(text, "    pub fn set_async_streaming(&mut self, enabled: bool) {\n",
                        GPU_DRAIN_METHODS + "    pub fn set_async_streaming(&mut self, enabled: bool) {\n",
                        "GPU queue barrier")


def instrument_verify(text):
    text = replace_once(text, '#[cfg(not(target_arch = "wasm32"))]\nfn main() {',
                        '#[cfg(not(target_arch = "wasm32"))]\nmod render_checks;\n'
                        '#[cfg(not(target_arch = "wasm32"))]\nfn main() {', "native verifier module")
    return replace_once(text, "    let check = std::env::args().nth(3);\n",
                        "    let check = std::env::args().nth(3);\n"
                        '    if check.as_deref() == Some("render-compare") {\n'
                        "        render_checks::run(seed, &dir, |_, _| {});\n"
                        "        return;\n"
                        "    }\n", "native fixture callback")


def baseline_harness(text):
    # The control has no lighting allocation API. Keep every other fixture and
    # timing statement identical to the candidate's harness.
    text, count = re.subn(r'"lightingBytes"\s*:\s*r\.lighting_bytes\(\)\s*,', "", text)
    if count != 1 or "lighting_bytes" in text:
        raise ValueError("render_checks.rs: expected exactly one lightingBytes report field")
    return text


def safe_extract(archive, destination):
    """Extract only ordinary Git archive entries, with no path traversal."""
    symlinks = []
    with tarfile.open(fileobj=io.BytesIO(archive), mode="r:") as tar:
        for member in tar.getmembers():
            path = PurePosixPath(member.name)
            if path.is_absolute() or ".." in path.parts or not path.parts:
                raise ValueError(f"Unsafe archive path: {member.name}")
            if path.parts[0] in (".tools", MARKER):
                raise ValueError(f"Archive already contains reserved preparation path: {member.name}")
            target = destination.joinpath(*path.parts)
            if member.isdir():
                target.mkdir(parents=True, exist_ok=True)
            elif member.isfile():
                target.parent.mkdir(parents=True, exist_ok=True)
                with tar.extractfile(member) as src, target.open("xb") as dst:
                    while chunk := src.read(1024 * 1024):
                        dst.write(chunk)
                target.chmod(member.mode & 0o777)
            elif member.issym():
                link = PurePosixPath(member.linkname)
                normalized = os.path.normpath(str(path.parent / link))
                if link.is_absolute() or normalized == ".." or normalized.startswith("../"):
                    raise ValueError(f"Unsafe archive symlink: {member.name} -> {member.linkname}")
                symlinks.append((target, member.linkname))
            else:
                raise ValueError(f"Unsupported Git archive entry: {member.name}")
    # Creating links last prevents extraction through a link from the archive.
    for target, link in symlinks:
        target.parent.mkdir(parents=True, exist_ok=True)
        target.symlink_to(link)


def file_sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def source_hashes(root):
    paths = sorted(path for path in (root / "src").rglob("*") if path.is_file())
    paths += [root / "Cargo.toml", root / "Cargo.lock"]
    return {str(path.relative_to(root)): file_sha(path) for path in paths}


def check_existing(existing, prepared):
    """Allow compiled WASM and the old target-path fix, but no engine edits."""
    failures = []
    for expected in sorted(prepared.rglob("*")):
        relative = expected.relative_to(prepared)
        if (expected.is_dir() and not expected.is_symlink()) or str(relative).startswith("dist/pkg/"):
            continue
        actual = existing / relative
        if not os.path.lexists(actual) or actual.is_symlink() != expected.is_symlink():
            failures.append(str(relative))
        elif expected.is_symlink():
            if os.readlink(actual) != os.readlink(expected):
                failures.append(str(relative))
        elif not actual.is_file():
            failures.append(str(relative))
        elif relative == Path("scripts/build.sh"):
            # The existing local control uses this build-path-only adaptation.
            normalized = actual.read_text().replace(
                '"${CARGO_TARGET_DIR:-target}/wasm32-unknown-unknown/release/fantasy_land.wasm"',
                "target/wasm32-unknown-unknown/release/fantasy_land.wasm")
            if normalized != expected.read_text():
                failures.append(str(relative))
        elif actual.read_bytes() != expected.read_bytes():
            pair = (file_sha(expected), file_sha(actual))
            if LEGACY_COMPATIBLE.get(str(relative)) != pair:
                failures.append(str(relative))
    # Additional engine modules could affect compilation even if the originals
    # remain intact. The one added fixture harness is part of the prepared tree.
    expected_src = {str(p.relative_to(prepared)) for p in (prepared / "src").rglob("*") if p.is_file()}
    actual_src = {str(p.relative_to(existing)) for p in (existing / "src").rglob("*") if p.is_file()}
    failures.extend(sorted(actual_src - expected_src))
    if failures:
        raise ValueError("Existing control differs from the preserved commit/hooks at: "
                         + ", ".join(sorted(set(failures))[:12]) + ". Use --force to prepare a fresh source backup.")


def print_build_instructions(source):
    target = source.parent / "target"
    q = shlex.quote
    print("\nBuild manually; preparation does not run Cargo, a browser, or the GPU:")
    # Keep the control's target directory and toolchain environment from leaking
    # into subsequent candidate builds in the same shell.
    print("(")
    print("set -eu")
    print(f"cd {q(str(source))}")
    print('if [ -x "$PWD/.tools/cargo/bin/cargo" ]; then')
    print('  export CARGO_HOME="$PWD/.tools/cargo" RUSTUP_HOME="$PWD/.tools/rustup"')
    print('  export PATH="$CARGO_HOME/bin:$PATH"')
    print("fi")
    print(f"export CARGO_TARGET_DIR={q(str(target))}")
    print("cargo build --release --bin verify --locked")
    print("cargo build --release --lib --target wasm32-unknown-unknown --locked")
    print('if [ -x "$PWD/.tools/wasm-bindgen-0.2.128-aarch64-apple-darwin/wasm-bindgen" ]; then')
    print('  baseline_bindgen="$PWD/.tools/wasm-bindgen-0.2.128-aarch64-apple-darwin/wasm-bindgen"')
    print("else")
    print("  baseline_bindgen=wasm-bindgen")
    print("fi")
    print('"$baseline_bindgen" --target web --out-dir dist/pkg --out-name fantasy_land '
          '"$CARGO_TARGET_DIR/wasm32-unknown-unknown/release/fantasy_land.wasm"')
    print(")")
    print(f"\nNative verifier: {target / 'release/verify'}")
    print("Capture output is a separate argument; choose a new directory to preserve earlier evidence.")


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--ref", default=DEFAULT_REF, help=f"original Git commit/ref (default: {DEFAULT_REF})")
    parser.add_argument("--output", type=Path, help="control source directory; default output/rendering-upgrade/baseline/source")
    parser.add_argument("--force", action="store_true", help="replace source only, preserving its previous contents as a sibling backup")
    parser.add_argument("--dry-run", action="store_true", help="validate an archive and hooks in a temporary directory without changing the control")
    args = parser.parse_args()
    repo = Path(__file__).resolve().parents[1]
    requested = args.output or repo / "output/rendering-upgrade/baseline/source"
    requested = Path(os.path.abspath(requested.expanduser()))
    if requested.is_symlink():
        raise ValueError("Output source cannot be a symlink")
    source = requested.parent.resolve() / requested.name
    if source == repo or source in repo.parents or (source / ".git").exists():
        raise ValueError("Output source must not replace a Git checkout or its ancestor")
    if ".git" in source.parts or ".tools" in source.parts:
        raise ValueError("Output source must not replace Git metadata or the shared toolchain")
    if repo in source.parents:
        tracked = subprocess.check_output(["git", "ls-files", "--", str(source.relative_to(repo))], cwd=repo, text=True)
        if tracked.strip():
            raise ValueError("Output source contains tracked repository files")
    if source.exists() and not source.is_dir():
        raise ValueError("Output source exists and is not a directory")
    sha = subprocess.check_output(["git", "rev-parse", "--verify", f"{args.ref}^{{commit}}"], cwd=repo, text=True).strip()
    archive = subprocess.check_output(["git", "archive", "--format=tar", sha], cwd=repo)
    harness = baseline_harness((repo / "src/render_checks.rs").read_text())
    with tempfile.TemporaryDirectory(prefix="fantasy-render-baseline-") as scratch:
        prepared = Path(scratch) / "source"
        prepared.mkdir()
        safe_extract(archive, prepared)
        for name, transform in [("lib.rs", instrument_lib), ("verify.rs", instrument_verify)]:
            path = prepared / "src" / name
            path.write_text(transform(path.read_text()))
        (prepared / "src/render_checks.rs").write_text(harness)
        if args.dry_run:
            print(f"Dry run passed: archive {sha}, guarded hooks and shared fixtures prepared; no output files changed.")
            print_build_instructions(source)
            return
        if source.exists() and not args.force:
            marker_path = source / MARKER
            if marker_path.exists():
                marker = json.loads(marker_path.read_text())
                if marker.get("commit") != sha:
                    raise ValueError("Existing control marker is for a different commit; use --force to keep it as a backup")
            check_existing(source, prepared)
            # This also adopts a verified manual control without rewriting it.
            action = "Reused verified control"
        else:
            source.parent.mkdir(parents=True, exist_ok=True)
            # Stage on the destination filesystem so the final rename is atomic.
            with tempfile.TemporaryDirectory(prefix=".prepare-render-baseline-", dir=source.parent) as local:
                staged = Path(local) / "source"
                shutil.copytree(prepared, staged, symlinks=True)
                if source.exists():
                    stamp = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%S%fZ")
                    backup = source.with_name(source.name + ".backup-" + stamp)
                    source.rename(backup)
                    print(f"Previous source preserved: {backup}")
                staged.rename(source)
            action = "Prepared control"
        tools = repo / ".tools"
        control_tools = source / ".tools"
        if tools.is_dir() and not os.path.lexists(control_tools):
            control_tools.symlink_to(tools.resolve(), target_is_directory=True)
        marker = {"schema": 1, "requestedRef": args.ref, "commit": sha,
                  "harnessSha256": file_sha(source / "src/render_checks.rs"),
                  "sourceSha256": source_hashes(source),
                  "changes": ["Game cameraPosition and asynchronous GPU drain", "native render-compare no-op callback", "shared fixture harness without lightingBytes"]}
        (source / MARKER).write_text(json.dumps(marker, indent=2) + "\n")
        print(f"{action}: {source}\nOriginal commit: {sha}\nProvenance marker: {source / MARKER}")
        print_build_instructions(source)


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        print(f"Baseline preparation failed: {error}", file=sys.stderr)
        sys.exit(1)
