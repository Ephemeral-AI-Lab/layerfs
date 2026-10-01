"""Standalone probe utilities; no LayerFS product dependencies."""
import datetime
import hashlib
import hmac
import http.client
import json
import os
from pathlib import Path
import random
import subprocess
import time
import urllib.parse
import xml.etree.ElementTree as ET

BLOCK = random.Random(290).randbytes(65536)
EMPTY_HASH = hashlib.sha256(b'').hexdigest()


def body_hash(size, block=BLOCK):
    digest = hashlib.sha256()
    for offset in range(0, size, len(block)):
        digest.update(block[:min(len(block), size - offset)])
    return digest.hexdigest()


def rss(pid=None):
    result = subprocess.run(['ps', '-o', 'rss=', '-p', str(pid or os.getpid())],
                            capture_output=True, text=True)
    return int(result.stdout.strip()) * 1024 if result.stdout.strip() else None


def write_json(path, value):
    Path(path).write_text(json.dumps(value, indent=2, sort_keys=True) + '\n')


def phase(record, label, operations, byte_count, function):
    before = rss()
    started = time.monotonic_ns()
    value = function()
    elapsed = time.monotonic_ns() - started
    record.setdefault('phases', []).append({
        'name': label, 'elapsed_ns': elapsed, 'operations': operations,
        'bytes': byte_count, 'ops_per_second': operations * 1e9 / elapsed,
        'mib_per_second': byte_count * 1e9 / elapsed / 1048576,
        'client_rss_before': before, 'client_rss_after': rss(),
        'memory_scope': 'current RSS snapshots; not a phase peak or physical bound',
    })
    return value


class S3:
    """Small fixed-count S3 SigV4 client; one reused loopback HTTP connection."""
    def __init__(self, config):
        self.config = config
        self.host = '127.0.0.1:' + str(config['port'])
        self.connection = http.client.HTTPConnection('127.0.0.1', config['port'], timeout=10)
        self.calls = 0
        self.sent = 0
        self.received = 0

    def call(self, method, key='', query=None, size=0, payload_hash=EMPTY_HASH,
             consume=False, verify_hash=False, block=BLOCK, request_headers=None, body_gate=None, data=None, response_limit=None):
        if data is not None and len(data) != size:
            raise ValueError('real request body size mismatch')
        path = '/' + self.config['bucket'] + ('/' + key if key else '')
        path = urllib.parse.quote(path, safe='/~')
        pairs = sorted((str(k), str(v)) for k, v in (query or {}).items())
        qs = '&'.join(urllib.parse.quote(k, safe='~') + '=' + urllib.parse.quote(v, safe='~')
                      for k, v in pairs)
        now = datetime.datetime.now(datetime.timezone.utc)
        stamp, date = now.strftime('%Y%m%dT%H%M%SZ'), now.strftime('%Y%m%d')
        signed = 'host;x-amz-content-sha256;x-amz-date'
        headers = f'host:{self.host}\nx-amz-content-sha256:{payload_hash}\nx-amz-date:{stamp}\n'
        canonical = '\n'.join([method, path, qs, headers, signed, payload_hash])
        scope = date + '/us-east-1/s3/aws4_request'
        sign = lambda key, text: hmac.new(key, text.encode(), hashlib.sha256).digest()
        signing = sign(sign(sign(sign(('AWS4' + self.config['secret']).encode(), date),
                                'us-east-1'), 's3'), 'aws4_request')
        string = '\n'.join(['AWS4-HMAC-SHA256', stamp, scope,
                            hashlib.sha256(canonical.encode()).hexdigest()])
        signature = hmac.new(signing, string.encode(), hashlib.sha256).hexdigest()
        auth = f"AWS4-HMAC-SHA256 Credential={self.config['access']}/{scope}, SignedHeaders={signed}, Signature={signature}"
        self.connection.putrequest(method, path + ('?' + qs if qs else ''),
                                   skip_host=True, skip_accept_encoding=True)
        for k, v in [('Host', self.host), ('x-amz-date', stamp),
                     ('x-amz-content-sha256', payload_hash), ('Authorization', auth),
                     ('Content-Length', str(size)), ('Connection', 'keep-alive')]:
            self.connection.putheader(k, v)
        for name, value in (request_headers or {}).items():
            self.connection.putheader(name, value)
        self.connection.endheaders()
        width = 65536 if data is not None else len(block)
        for offset in range(0, size, width):
            self.connection.send(data[offset:offset + width] if data is not None
                                 else block[:min(width, size - offset)])
            if offset == 0 and body_gate is not None:
                body_gate()
        response = self.connection.getresponse()
        self.calls += 1
        self.sent += size
        if response.status not in (200, 204, 206):
            content = response.read(8192)
            try:
                code = ET.fromstring(content).findtext('Code')
            except ET.ParseError:
                code = 'unparsed error'
            raise RuntimeError(f'{method} status={response.status} code={code}')
        if request_headers and 'Range' in request_headers and response.status != 206:
            raise RuntimeError('range request did not return 206')
        if consume:
            count, digest = 0, hashlib.sha256() if verify_hash else None
            while True:
                data = response.read(65536)
                if not data:
                    break
                count += len(data)
                if digest is not None:
                    digest.update(data)
            self.received += count
            return count, digest.hexdigest() if digest else None
        data = response.read() if response_limit is None else response.read(response_limit + 1)
        if response_limit is not None and len(data) > response_limit:
            raise RuntimeError('response body exceeds declared admission')
        self.received += len(data)
        return data

    def listing(self, prefix):
        token, keys, calls = None, [], 0
        while True:
            query = {'list-type': 2, 'prefix': prefix, 'max-keys': 1000}
            if token:
                query['continuation-token'] = token
            doc = ET.fromstring(self.call('GET', query=query))
            ns = {'s': 'http://s3.amazonaws.com/doc/2006-03-01/'}
            keys.extend(e.findtext('s:Key', namespaces=ns)
                        for e in doc.findall('s:Contents', ns))
            calls += 1
            if doc.findtext('s:IsTruncated', namespaces=ns) != 'true':
                return keys, calls
            token = doc.findtext('s:NextContinuationToken', namespaces=ns)
            if not token:
                raise RuntimeError('truncated listing without continuation')

    def close(self):
        self.connection.close()
