"""Scan local drives for singing-synthesis project files.

Writes JSONL (one file per line) to bench-night/scan.jsonl so partial progress
survives an interrupted run. Prunes build/tool directories aggressively; this is
an IO sweep, not a full-disk index.
"""

from __future__ import annotations

import json
import os
import sys
import time
from pathlib import Path

ROOTS = [
    r"C:\Users\newztl\Downloads",
    r"C:\Users\newztl\Documents",
    r"C:\Users\newztl\Desktop",
    r"C:\Users\newztl\Music",
    r"D:\Documents",
    r"D:\Downloads",
    r"D:\linshizhuomian",
    r"D:\prw",
    r"D:\repos",
    r"D:\QQ",
    r"D:\BaiduNetdisk",
    r"E:\2024V",
    r"E:\2026V吧赛",
    r"E:\2026歌影",
    r"E:\AAAbysjsx",
    r"E:\AI音乐制作",
    r"E:\BaiduNetdiskDownload",
    r"E:\CloudMusic",
    r"E:\Cakewalk Projects",
    r"E:\bb",
    r"E:\xs",
]

PRUNE = {
    "windows", "program files", "program files (x86)", "programdata", "appdata",
    "node_modules", ".git", "target", ".refs", ".pnpm-store", "$recycle.bin",
    "system volume information", ".venv", "venv", "site-packages", "miniconda",
    "miniconda3", ".cargo", ".rustup", ".gradle", ".konan", "dist", "build",
    "__pycache__", ".next", ".cache", "temp", "tmp",
}

EXTS = {
    ".ustx", ".svp", ".vsqx", ".vsq", ".ust", ".mid", ".midi", ".musicxml",
    ".mxl", ".ppsf", ".tlp", ".tlpx", ".ccs", ".svip", ".svip3", ".acep",
    ".vpr", ".vxf", ".ufdata", ".aisp", ".y77", ".vsp", ".gj", ".msq",
    ".nn", ".s5p", ".dspx", ".dv", ".vog", ".vfp", ".vshp", ".vspx", ".xvsq",
}

MIN_SIZE = 4 * 1024
MAX_SIZE = 80 * 1024 * 1024


def main() -> int:
    out_path = Path(r"E:\RUST_LIBRESVIP\.tmp\bench-night\scan.jsonl")
    out_path.parent.mkdir(parents=True, exist_ok=True)
    seen = set()
    if out_path.exists():
        for line in out_path.read_text(encoding="utf-8", errors="replace").splitlines():
            try:
                seen.add(json.loads(line)["path"])
            except Exception:
                pass
    found = 0
    started = time.time()
    with out_path.open("a", encoding="utf-8") as out:
        for root in ROOTS:
            if not os.path.isdir(root):
                continue
            for dirpath, dirnames, filenames in os.walk(root):
                dirnames[:] = [d for d in dirnames if d.lower() not in PRUNE]
                for name in filenames:
                    suffix = os.path.splitext(name)[1].lower()
                    if suffix not in EXTS:
                        continue
                    path = os.path.join(dirpath, name)
                    if path in seen:
                        continue
                    try:
                        stat = os.stat(path)
                    except OSError:
                        continue
                    if not (MIN_SIZE <= stat.st_size <= MAX_SIZE):
                        continue
                    record = {
                        "path": path,
                        "ext": suffix,
                        "size": stat.st_size,
                        "mtime": int(stat.st_mtime),
                    }
                    out.write(json.dumps(record, ensure_ascii=False) + "\n")
                    out.flush()
                    seen.add(path)
                    found += 1
                    if found % 50 == 0:
                        print(f"{found} files, {time.time()-started:.0f}s", flush=True)
                if time.time() - started > 2700:
                    print("time cap reached", flush=True)
                    return 0
    print(f"done: {found} new files in {time.time()-started:.0f}s", flush=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())
