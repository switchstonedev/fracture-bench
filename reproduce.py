#!/usr/bin/env python3
"""
Root entrypoint delegator for Fracture benchmark reproduction.
Directs execution to scripts/reproduce.py.
"""
import sys
from pathlib import Path

root = Path(__file__).resolve().parent
scripts_reproduce = root / "scripts" / "reproduce.py"
if not scripts_reproduce.is_file():
    scripts_reproduce = root / "Scripts" / "reproduce.py"
if not scripts_reproduce.is_file():
    raise FileNotFoundError(f"Underlying runner not found: {scripts_reproduce}")

with open(scripts_reproduce, "rb") as f:
    code = compile(f.read(), str(scripts_reproduce), "exec")
    sys.argv[0] = str(scripts_reproduce)
    exec(code, {"__name__": "__main__", "__file__": str(scripts_reproduce)})
