"""Development-tool contracts; no product or live service work in unit tests."""

from datetime import datetime, timezone
import json
from pathlib import Path
import re
import subprocess
from tempfile import TemporaryDirectory
import unittest
from unittest.mock import patch, MagicMock
from http.client import RemoteDisconnected

import phase7_services as services


class Phase7ServicesTests(unittest.TestCase):
    def test_both_images_are_immutable_pins(self):
        for image in (services.POSTGRES_IMAGE, services.MINIO_IMAGE):
            self.assertRegex(image, r'^[^@]+@sha256:[0-9a-f]{64}$')

    def test_absent_object_is_distinct_from_daemon_failure(self):
        missing = subprocess.CompletedProcess([], 1, '', 'error: no such object: layerfs-postgres')
        with patch.object(services, 'docker', return_value=missing):
            self.assertIsNone(services.inspect('container', 'layerfs-postgres'))
        network = subprocess.CompletedProcess([], 1, '', 'Error response from daemon: network layerfs-phase7-cluster1 not found')
        with patch.object(services, 'docker', return_value=network):
            self.assertIsNone(services.inspect('network', 'layerfs-phase7-cluster1'))
        unavailable = subprocess.CompletedProcess([], 1, '', 'Cannot connect to the Docker daemon')
        with patch.object(services, 'docker', return_value=unavailable):
            with self.assertRaisesRegex(RuntimeError, 'cannot inspect'):
                services.inspect('container', 'layerfs-postgres')

    def test_foreign_resource_is_refused_before_any_removal(self):
        foreign = {'Config': {'Labels': {services.OWNER_LABEL: '/another/worktree'}}}
        with patch.object(services, 'inspect', return_value=foreign), patch.object(services, 'docker') as docker:
            with self.assertRaisesRegex(RuntimeError, 'foreign container'):
                services.down()
            docker.assert_not_called()

    def test_partial_owned_setup_is_never_resumed_silently(self):
        inventory = {'container': {'pg': {'State': {'Running': True}}, 'minio': None},
                     'volume': {}, 'network': {}}
        with patch.object(services, 'ownership_inventory', return_value=inventory), patch.object(services, 'load', return_value={'complete': False}), patch.object(services, 'docker') as docker:
            with self.assertRaisesRegex(RuntimeError, 'partial owned setup'):
                services.up()
            docker.assert_not_called()

    def test_credentials_stay_in_private_files(self):
        settings = services.fresh_settings()
        with TemporaryDirectory() as tmp:
            path = Path(tmp) / 'settings.json'
            with patch.object(services, 'STATE', path):
                services.save(settings)
                self.assertEqual(path.stat().st_mode & 0o777, 0o600)
                self.assertEqual(path.with_name('services.env').stat().st_mode & 0o777, 0o600)
                self.assertEqual(services.load()['environment'], settings['environment'])
        self.assertNotEqual(settings['environment']['LAYERFS_PG_PASSWORD'], settings['environment']['LAYERFS_S3_SECRET_KEY'])

    def test_bucket_signature_has_the_declared_body_and_date(self):
        environment = services.fresh_settings()['environment']
        request = services.signed_bucket_request(environment, datetime(2026, 10, 3, 12, 0, tzinfo=timezone.utc))
        headers = {key.lower(): value for key, value in request.header_items()}
        self.assertEqual(request.method, 'PUT')
        self.assertEqual(request.full_url, 'http://127.0.0.1:9000/layerfs')
        self.assertEqual(request.data, b'')
        self.assertEqual(headers['x-amz-date'], '20261003T120000Z')
        self.assertRegex(headers['authorization'], r'/20261003/us-east-1/s3/aws4_request, SignedHeaders=host;x-amz-content-sha256;x-amz-date, Signature=[0-9a-f]{64}$')
        self.assertNotIn(environment['LAYERFS_S3_SECRET_KEY'], headers['authorization'])

    def test_readiness_handles_startup_close_and_checks_tcp(self):
        response = MagicMock(); response.__enter__.return_value.status = 200
        ready = subprocess.CompletedProcess([], 0, '', '')
        with patch.object(services, 'docker', return_value=ready) as docker, patch.object(services.urllib.request, 'urlopen', side_effect=[RemoteDisconnected('startup'), response]), patch.object(services, 'owned', return_value={'State': {'Running': True}}), patch.object(services.time, 'sleep'):
            services.wait_ready({})
            self.assertIn('-h', docker.call_args.args)
            self.assertIn('127.0.0.1', docker.call_args.args)

    def test_private_credentials_are_omitted_from_status(self):
        settings = services.fresh_settings(); settings['image_ids'] = {'postgres': 'id-pg', 'minio': 'id-minio'}
        version = subprocess.CompletedProcess([], 0, 'minio version RELEASE.test\n', '')
        with patch.object(services, 'validate'), patch.object(services, 'postgres_sql', return_value='fsync=on\nsynchronous_commit=on\n'), patch.object(services, 'docker', return_value=version):
            result = services.status(settings)
        text = json.dumps(result)
        for key in ('LAYERFS_PG_PASSWORD', 'LAYERFS_S3_SECRET_KEY'):
            self.assertNotIn(settings['environment'][key], text)
        self.assertEqual(result['resource_profile']['memory_bytes_per_service'], 512 * 1024 * 1024)

    def test_cleanup_removes_only_verified_owned_names(self):
        inventory = {'container': {'pg': {}}, 'volume': {'pg-data': {}}, 'network': {'private-net': {}}}
        with TemporaryDirectory() as tmp, patch.object(services, 'STATE', Path(tmp) / 'absent'), patch.object(services, 'ownership_inventory', return_value=inventory), patch.object(services, 'docker') as docker:
            services.down()
            self.assertEqual([call.args for call in docker.call_args_list], [('rm', '--force', 'pg'), ('volume', 'rm', 'pg-data'), ('network', 'rm', 'private-net')])


if __name__ == '__main__':
    unittest.main()
