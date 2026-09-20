#!/usr/bin/env python3
"""Use unchanged retained-history arithmetic on this matched pair."""
import importlib.util
from pathlib import Path
p=Path(__file__).resolve().parent
spec=importlib.util.spec_from_file_location("h190_analysis",p.parent/"stage-6-history-190-opt-20260919T232858Z/analyze_results.py")
analysis=importlib.util.module_from_spec(spec)
spec.loader.exec_module(analysis)
analysis.ROOT=p
analysis.main()
