"""Count-driven remainder correctness proof; never a new speed arm."""
import argparse
import hashlib
import json
from pathlib import Path
import sqlite3

from common import write_json
from repository_probe import deadline_command, sha256_file
from run import inventory


def run(args):
    original = Path(args.original).resolve()
    output = Path(args.output).resolve(); output.mkdir(exist_ok=False)
    source = inventory(); assert not source['tracked_dirty']
    stage = original / 'stage'
    binary = Path(args.binary).resolve()
    progress = json.loads((stage / 'reconstruction-progress.json').read_text())
    assert progress == {'files': 13312, 'bytes': 1166388570, 'metadata': 'PASS', 'complete': False}
    receipt = json.loads((original / 'verification-command.json').read_text())
    assert receipt['status'] == 'TIMEOUT' and receipt['limit_seconds'] == 10
    transfer = json.loads((original / 'download.json').read_text())
    assert transfer['packs'] == 5836 and transfer['bytes'] == 1389188326
    write_json(output / 'identity.json', {'source': source,
               'binary_sha256': sha256_file(binary), 'catalog_sha256': sha256_file(stage/'catalog.sqlite'),
               'master_manifest_sha256': sha256_file(Path(args.master)/'manifest.sqlite'),
               'reused_original_proof': sha256_file(original/'verification-command.json'),
               'reused_progress': sha256_file(stage/'reconstruction-progress.json'),
               'reused_download_ack_transcript': sha256_file(original/'download-acks.jsonl'),
               'performance_sample': False})
    db = sqlite3.connect(f'file:{stage / "catalog.sqlite"}?mode=ro', uri=True)
    # One ordinal cursor derivation; later queries always keyset, never repeated OFFSET.
    cursor = db.execute("SELECT path FROM entries WHERE kind='file' ORDER BY path LIMIT 1 OFFSET ?",
                        (progress['files']-1,)).fetchone()[0]
    expected_files, expected_bytes = db.execute("SELECT count(*),sum(size) FROM entries WHERE kind='file'").fetchone()
    db.close()
    cursor_path = output/'initial-cursor.bin'; cursor_path.write_bytes(cursor)
    files, byte_count = progress['files'], progress['bytes']
    batches = []
    index = 0
    while files < expected_files:
        count = min(4096, expected_files-files)
        destination = output/f'batch-{index:02d}'; destination.mkdir()
        argv = [str(binary),'verify-batch',str(Path(args.master).resolve()),str(stage),
                str(original/'downloaded-packs'),str(cursor_path),str(count),str(destination)]
        result = deadline_command(argv,destination/'proof.log',10)
        write_json(destination/'command.json',result)
        if result['status'] != 'COMPLETE':
            write_json(output/'aggregate.json',{'status':'INCOMPLETE','files':files,
                       'bytes':byte_count,'failed_batch':index,'original_phase':'TIMEOUT'})
            return
        proof = json.loads((destination/'batch-proof.json').read_text())
        assert proof['proof'] == 'PASS' and proof['files'] == count and proof['exact_eof']
        next_path = destination/'next-cursor.bin'; next_cursor = next_path.read_bytes()
        assert next_cursor > cursor
        cursor = next_cursor; cursor_path = next_path
        files += proof['files']; byte_count += proof['bytes']
        batches.append({'index':index,'files':proof['files'],'bytes':proof['bytes'],
                        'command_wall_ns':result['wall_ns']})
        print('batch',index,'PASS','total_files',files,flush=True)
        index += 1
    assert files == 103108 == expected_files and byte_count == 3475776149 == expected_bytes
    write_json(output/'aggregate.json',{'status':'EXHAUSTIVE_CORRECTNESS_PASS',
               'files':files,'bytes':byte_count,'reused_files':progress['files'],
               'reused_bytes':progress['bytes'],'batches':batches,
               'original_phase':'TIMEOUT','performance_sample':False})


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--original',default='benchmark-results/storage-probes/deepseek-full-import-v1')
    parser.add_argument('--master',default='benchmark-results/storage-probes/deepseek-full-master-v2')
    parser.add_argument('--binary',default='core/target/release/examples/minio_repository_probe')
    parser.add_argument('--output',required=True)
    run(parser.parse_args())
