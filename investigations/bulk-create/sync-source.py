#!/usr/bin/env python3
"""Update a private build tree without invalidating unchanged Cargo inputs."""
from pathlib import Path
import shutil,sys
source,target=map(Path,sys.argv[1:])
for path in source.rglob('*'):
    if not path.is_file(): continue
    destination=target/path.relative_to(source)
    if not destination.exists() or path.read_bytes()!=destination.read_bytes():
        destination.parent.mkdir(parents=True,exist_ok=True)
        shutil.copy2(path,destination)
        print(path.relative_to(source))
