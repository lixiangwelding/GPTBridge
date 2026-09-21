#!/usr/bin/env python3
"""Repeatable personal-fork tests with isolated state and per-stage durable receipts."""
from __future__ import annotations
import argparse
import hashlib
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import time
from personal_env import toolchain_environment, toolchain_paths

ROOT = Path(__file__).resolve().parents[1]
SUITES: dict[str, list[tuple[str, list[str], int]]] = {
    "scripts": [("python-scripts",[sys.executable,"-m","unittest","discover","-s","tests/personal","-p","test_*.py","-v"],60)],
    "runtime": [("runtime",["cargo","test","--manifest-path","personal-runtime/Cargo.toml","--","--nocapture"],300)],
    "mcp": [
        ("worker-build",["cargo","build","--manifest-path","personal-runtime/Cargo.toml"],300),
        ("mcp-contract",["cargo","test","--manifest-path","src-tauri/Cargo.toml","--lib","personal_tests","--","--nocapture"],480),
        ("mcp-protocol",["cargo","test","--manifest-path","src-tauri/Cargo.toml","--lib","mcp::server::tests"],300),
        ("policy",["cargo","test","--manifest-path","src-tauri/Cargo.toml","--lib","tools::policy::tests"],300),
        ("registry",["cargo","test","--manifest-path","src-tauri/Cargo.toml","--lib","tools::registry::tests"],300),
        ("full-rust-regression",["cargo","test","--manifest-path","src-tauri/Cargo.toml","--lib"],480),
    ],
    "frontend": [
        ("prompt-ui",["node","--test","tests/chatgpt-session-prompt-layout.test.mjs"],60),
        ("svelte-check",["npm","run","check"],180),
        ("frontend-build",["npm","run","build"],180),
    ],
    "build": [("desktop-build",["cargo","build","--manifest-path","src-tauri/Cargo.toml"],480)],
}

def fingerprint() -> str:
    digest = hashlib.sha256()
    for directory in ("personal-runtime/src","personal-runtime/tests","src-tauri/src","src","scripts","tests"):
        for path in sorted((ROOT/directory).rglob("*")):
            if path.is_file() and "__pycache__" not in path.parts and path.suffix in {".rs",".py",".ts",".svelte",".mjs",".js"}:
                digest.update(str(path.relative_to(ROOT)).encode()); digest.update(path.read_bytes())
    for name in ("personal-runtime/Cargo.toml","personal-runtime/Cargo.lock","src-tauri/Cargo.toml","src-tauri/Cargo.lock","src-tauri/tauri.conf.json","package.json","package-lock.json"):
        path=ROOT/name
        if path.is_file(): digest.update(name.encode()); digest.update(path.read_bytes())
    return digest.hexdigest()

def save(path: Path, value: dict) -> None:
    temp=path.with_suffix(".tmp")
    with temp.open("w",encoding="utf-8") as stream:
        json.dump(value,stream,ensure_ascii=False,indent=2); stream.flush(); os.fsync(stream.fileno())
    os.replace(temp,path)

def main(argv: list[str] | None = None) -> int:
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--suite",choices=["all",*SUITES],default="all")
    parser.add_argument("--resume",type=Path,help="resume a matching-source report; never skips changed inputs")
    args=parser.parse_args(argv)
    before=fingerprint()
    if args.resume:
        report_path=args.resume.resolve()
        if not report_path.is_relative_to(ROOT/".artifacts"/"selftest"):
            parser.error("resume must point to this repository's private selftest directory")
        report=json.loads(report_path.read_text())
        if report["source_sha256"]!=before or report["suite"]!=args.suite:
            parser.error("source/suite changed; start a new run instead of reusing old success")
        directory=report_path.parent
    else:
        directory=ROOT/".artifacts"/"selftest"/str(time.time_ns()); directory.mkdir(parents=True,mode=0o700)
        report_path=directory/"result.json"
        report={"suite":args.suite,"source_sha256":before,"stages":{},"started":time.time(),"old_service_modified":False}
    env=toolchain_environment()
    report["toolchain"]=toolchain_paths(env)
    worker="coding-tools-personal-worker"+(".exe" if os.name=="nt" else "")
    env.update({"CODING_TOOLS_PERSONAL_HOME":str(directory/"home"),"CODING_TOOLS_PERSONAL_IMPORT":"off",
        "CODING_TOOLS_PERSONAL_WORKER":str(ROOT/".artifacts"/"cargo"/"debug"/worker),
        "CARGO_TARGET_DIR":str(ROOT/".artifacts"/"cargo"),"CARGO_BUILD_JOBS":"2","RUST_TEST_THREADS":"2"})
    stages=[stage for key,values in SUITES.items() if args.suite in {"all",key} for stage in values]
    report["status"]="running"; save(report_path,report)
    for name,command,timeout in stages:
        if report["stages"].get(name,{}).get("status")=="passed": continue
        attempt=directory/f"{name}-{time.time_ns()}"; attempt.mkdir()
        record={"command":command,"status":"running","started":time.time(),"logs":str(attempt)}
        report["stages"][name]=record; save(report_path,report)
        with (attempt/"stdout.log").open("wb") as stdout, (attempt/"stderr.log").open("wb") as stderr:
            process=None
            try:
                process=subprocess.Popen(command,cwd=ROOT,env=env,stdout=stdout,stderr=stderr,start_new_session=True)
                code=process.wait(timeout=timeout)
            except (subprocess.TimeoutExpired,KeyboardInterrupt):
                if process is not None:
                    if os.name=="posix": os.killpg(process.pid,signal.SIGTERM)
                    else: process.terminate()
                    try: process.wait(timeout=5)
                    except subprocess.TimeoutExpired:
                        if os.name=="posix": os.killpg(process.pid,signal.SIGKILL)
                        else: process.kill()
                        process.wait()
                code=124
            except OSError as error:
                stderr.write(str(error).encode()); code=127
        record.update(exit_code=code,status="passed" if code==0 else "failed",finished=time.time())
        save(report_path,report)
        print(json.dumps({"stage":name,"exit_code":code,"report":str(report_path)}),flush=True)
        if code:
            report["status"]="failed"; save(report_path,report)
            print((attempt/"stderr.log").read_text(errors="replace")[-14000:])
            print((attempt/"stdout.log").read_text(errors="replace")[-14000:])
            return code
    after=fingerprint(); report.update(source_sha256_after=after,finished=time.time())
    report["status"]="passed" if before==after else "input_drift"
    save(report_path,report)
    print(json.dumps({"status":report["status"],"report":str(report_path),"source_sha256":before}),flush=True)
    return 0 if before==after else 3

if __name__=="__main__":
    raise SystemExit(main())
