#!/usr/bin/env python3
"""Generate Rust and Go from the pinned, checked-in protobuf import closure."""
import argparse
import filecmp
import json
from pathlib import Path
import re
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
MANIFEST = json.loads((ROOT / "proto/codegen.json").read_text())


def run(*args, cwd=ROOT):
    subprocess.run(args, cwd=cwd, check=True)


def sources():
    found = {}
    pending = list(MANIFEST["roots"])
    while pending:
        name = pending.pop()
        if name in found:
            continue
        path = next((base / name for base in (ROOT / "proto/shieldd", ROOT / "proto/rust-vendored", Path(shutil.which("protoc")).resolve().parent.parent / "include") if (base / name).is_file()), None)
        if path is None:
            raise SystemExit(f"missing pinned schema: {name}")
        found[name] = path
        pending.extend(re.findall(r'^import\s+(?:(?:public|weak)\s+)?"([^"]+)"', path.read_text(), re.MULTILINE))
    return found


def identical(left, right):
    if not right.is_dir():
        return False
    comparison = filecmp.dircmp(left, right)
    return not (comparison.left_only or comparison.right_only or comparison.funny_files) and all(
        filecmp.cmp(left / name, right / name, shallow=False) for name in comparison.common_files
    ) and all(identical(left / name, right / name) for name in comparison.common_dirs)


def filter_rust_rpc(text, package):
    """Retain only RPC implementations used by the Rust runtime and tools."""
    pattern = r"^/// Generated (client|server) implementations\.\n.*?(?=^/// Generated (?:client|server) implementations\.\n|\Z)"
    def retain(match):
        allowed = MANIFEST[f"rust_rpc_{match[1]}s"]
        return match[0] if package in allowed else ""
    return re.sub(pattern, retain, text, flags=re.MULTILINE | re.DOTALL)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="compare without changing tracked files")
    args = parser.parse_args()
    version = subprocess.check_output(["protoc", "--version"], text=True).strip()
    if version != "libprotoc " + MANIFEST["protoc_version"]:
        raise SystemExit(f"expected protoc {MANIFEST['protoc_version']}, got {version}")
    closure = sources()
    with tempfile.TemporaryDirectory(prefix="shieldd-proto-") as temporary:
        stage = Path(temporary)
        inputs, rust, go, vendor = (stage / name for name in ("inputs", "rust", "go/gen", "vendor"))
        for directory in (inputs, rust, go, vendor):
            directory.mkdir(parents=True)
        for name, source in sorted(closure.items()):
            destination = inputs / name
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(source, destination)
            if not name.startswith("shieldd/"):
                destination = vendor / name
                destination.parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(source, destination)
        run("cargo", "run", "--quiet", "--locked", "--manifest-path", str(ROOT / "tools/proto-compiler/Cargo.toml"), "--", str(inputs), str(rust), *MANIFEST["roots"])
        packages = set(MANIFEST["rust_packages"])
        for path in rust.glob("*.rs"):
            if path.name.removesuffix(".rs").removesuffix(".serde") not in packages:
                path.unlink()
            else:
                path.write_text("\n".join(line.rstrip() for line in filter_rust_rpc(path.read_text(), path.stem).splitlines()) + "\n")
        for package in packages:
            if not (rust / f"{package}.rs").is_file():
                raise SystemExit(f"missing generated Rust package {package}")
        plugin = stage / "protoc-gen-go"
        run("go", "build", "-p", "2", "-mod=readonly", "-o", str(plugin), "google.golang.org/protobuf/cmd/protoc-gen-go", cwd=ROOT / "tools/proto-compiler")
        go_sources = sorted(name for name in closure if not name.startswith("google/protobuf/"))
        mappings = [f"--go_opt=M{name}=github.com/mizufinance/shieldd/proto/go/gen/{Path(name).parent.as_posix()}" for name in go_sources]
        mappings.extend(f"--go_opt=M{name}=google.golang.org/protobuf/types/{'descriptorpb' if Path(name).stem == 'descriptor' else 'known/' + Path(name).stem.replace('_', '') + 'pb'}" for name in sorted(closure) if name.startswith("google/protobuf/"))
        run("protoc", f"-I{inputs}", f"--plugin=protoc-gen-go={plugin}", f"--go_out={go}", "--go_opt=paths=source_relative", *mappings, *go_sources)
        for name in ("go.mod", "go.sum"):
            shutil.copyfile(ROOT / "proto/go" / name, go.parent / name)
        run("go", "test", "-p", "2", "-mod=readonly", "./...", cwd=go.parent)
        outputs = [(rust, ROOT / "crates/proto/src/gen"), (vendor, ROOT / "proto/rust-vendored")]
        if not args.check:
            outputs.append((go, ROOT / "proto/go/gen"))
        changed = [str(destination.relative_to(ROOT)) for source, destination in outputs if not identical(source, destination)]
        if args.check:
            if changed:
                raise SystemExit("generated protobuf drift: " + ", ".join(changed))
        else:
            for source, destination in outputs:
                if str(destination.relative_to(ROOT)) not in changed:
                    continue
                if destination.exists():
                    shutil.rmtree(destination)
                shutil.copytree(source, destination)
        print(f"Verified {len(closure)} schemas and {len(packages)} Rust packages; Rust and Go generation complete.")


if __name__ == "__main__":
    main()
