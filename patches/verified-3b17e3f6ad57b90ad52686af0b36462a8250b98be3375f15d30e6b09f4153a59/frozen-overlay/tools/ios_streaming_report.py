#!/usr/bin/env python3
"""Summarize native iOS console windows without inventing aggregate percentiles.

Usage: python3 tools/ios_streaming_report.py CONSOLE.log > summary.json
Boundary windows are retained in `windows` but excluded from scenario summaries.
Resident/peak numbers are sampled process residency, not OS lifetime peaks.
"""
import json
import re
import sys
from pathlib import Path


def report(text):
    phase = "loading"
    boundary = True
    pending = None
    windows = []
    startup = []
    imports = []
    centers = set()
    lifecycle = []
    for line in text.splitlines():
        if "Horizon iOS suspended" in line or "Horizon iOS resumed" in line:
            lifecycle.append(line.strip())
            boundary = True
            pending = None
        if "STARTUP " in line:
            startup.append(line.strip())
        if "RESOURCE_IMPORT " in line:
            imports.append(line.strip())
        match = re.search(r"BENCH phase=(\d+)", line)
        if match:
            phase = ["static", "movement", "rapid", "return"][int(match[1])]
            boundary = True
        match = re.search(r"STREAM center=\((-?\d+), (-?\d+)\)", line)
        if match:
            centers.add(tuple(map(int, match.groups())))
        if "TERRAIN frames=300" in line:
            pairs = re.findall(r"(fps|p50_ms|p95_ms|p99_ms|max_ms)=([\d.]+)", line)
            pending = dict((key, float(value)) for key, value in pairs)
            pending.update(phase=phase, boundary=boundary)
            boundary = False
            match = re.search(r"resident_bytes=Some\((\d+)\)", line)
            if match:
                pending["resident_mib"] = int(match[1]) / 1048576
        if "FRAME_WORK " in line and pending is not None:
            for key in ["sampled_peak_bytes", "chunks", "sections", "triangles"]:
                match = re.search(rf"{key}=(\d+)", line)
                if match:
                    pending[key] = int(match[1])
            windows.append(pending)
            pending = None
    scenarios = {}
    for name in ["static", "movement", "rapid", "return"]:
        rows = [w for w in windows if w["phase"] == name and not w["boundary"]]
        if not rows:
            continue
        ranges = {}
        for key in ["fps", "p95_ms", "p99_ms", "max_ms", "resident_mib", "chunks", "sections", "triangles"]:
            values = [w[key] for w in rows if key in w]
            if values:
                ranges[key] = [min(values), max(values)]
        scenarios[name] = {"windows": len(rows), "seconds": sum(300/w["fps"] for w in rows),
                           "fps": len(rows)*300/sum(300/w["fps"] for w in rows),
                           "window_ranges": ranges}
    return {"startup": startup, "imports": imports, "observed_centers": len(centers), "lifecycle": lifecycle,
            "scenarios": scenarios, "windows": windows,
            "sampled_peak_mib": max((w.get("sampled_peak_bytes", 0) for w in windows), default=0)/1048576}


if __name__ == "__main__":
    print(json.dumps(report(Path(sys.argv[1]).read_text()), indent=2))
