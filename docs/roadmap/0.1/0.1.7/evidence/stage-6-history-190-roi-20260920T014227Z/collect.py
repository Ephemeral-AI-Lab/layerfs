#!/usr/bin/env python3
"""Reuse the previous append-only collector for this campaign."""
import importlib.util
from pathlib import Path
p = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("h190_collector", p.parent / "stage-6-history-190-opt-20260919T232858Z/collect_diagnostic.py")
collector = importlib.util.module_from_spec(spec)
spec.loader.exec_module(collector)
collector.CAMPAIGN = p
collector.main()
