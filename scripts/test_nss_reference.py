# Copyright (C) 2026 Open Source Robotics Foundation
# SPDX-License-Identifier: Apache-2.0
import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location('nss_reference', Path(__file__).with_name('nss-reference.py'))
nss = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(nss)


class NssReferencePolicy(unittest.TestCase):
    def config(self):
        return {'image': 'ubuntu@sha256:' + 'a'*64, 'platform': 'linux/amd64', 'glibc': '2.39'}

    def test_only_exact_platform_digest_and_immutable_image_id_are_admitted(self):
        identity = 'sha256:' + 'b'*64
        metadata = {'Id': identity, 'Os': 'linux', 'Architecture': 'amd64', 'RepoDigests': [self.config()['image']]}
        with patch.object(nss.subprocess, 'check_output', return_value=json.dumps([metadata]).encode()):
            self.assertEqual(nss.image_identity(self.config()), identity)
            for key, value in [('image', 'ubuntu:24.04'), ('platform', 'linux/arm64'), ('glibc', '2.40')]:
                with self.subTest(config=key), self.assertRaises(ValueError):
                    nss.image_identity(dict(self.config(), **{key: value}))
        for key, value in [('Id', 'ubuntu:24.04'), ('Architecture', 'arm64'), ('RepoDigests', ['ubuntu@sha256:'+'c'*64])]:
            with patch.object(nss.subprocess, 'check_output', return_value=json.dumps([dict(metadata, **{key: value})]).encode()), self.assertRaises(ValueError):
                nss.image_identity(self.config())

    def test_actual_status_order_and_explicit_bound_difference_are_required(self):
        case = {'name': 'ordering', 'expected': ['203.0.113.9', '198.51.100.2'], 'bounded': False}
        good = 'GLIBC 2.39\nLEGACY 1 1 0 203.0.113.9 198.51.100.2\nREENTRANT 0 1 1 203.0.113.9 198.51.100.2\n'
        self.assertEqual(nss.expected_result(case, good), 'Ok(Some(203.0.113.9))')
        for bad in [good.replace('2.39', '2.40'), good.replace('REENTRANT 0 1 1 203.0.113.9 198.51.100.2', 'REENTRANT 0 1 1 198.51.100.2 203.0.113.9'), 'missing\n']:
            with self.assertRaises(ValueError):
                nss.expected_result(case, bad)
        bounded = dict(case, bounded=True)
        actual = good.replace('REENTRANT 0 1 1 203.0.113.9 198.51.100.2', 'REENTRANT 34 0 -1')
        self.assertEqual(nss.expected_result(bounded, actual), 'Err(BufferLimit)')
        with self.assertRaises(ValueError):
            nss.expected_result(bounded, good)

    def test_every_child_uses_prebatch_id_and_changed_digest_binding_fails(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root/'tests/fixtures').mkdir(parents=True)
            (root/'artifacts').mkdir()
            cases = [dict(name=str(i), hosts='', query=str(i), nss='hosts: files\n', host_conf='multi on\n', expected=[], bounded=i == 21) for i in range(22)]
            cases[-1]['expected'] = ['203.0.113.9']
            (root/'tests/fixtures/nss.json').write_text(json.dumps({'cases': cases}))
            peer = root/'test-executable'
            peer.write_bytes(b'test')
            (root/'artifacts/nss-peer').write_bytes(b'native')
            identity = 'sha256:' + 'b'*64
            def run(command, **_):
                native = str(root/'artifacts/nss-peer') in command
                query = command[-1] if native else next(value.split('=', 1)[1] for value in command if value.startswith('GZ_NSS_CHILD_NAMES='))
                bounded = query == '21'
                if native:
                    stdout = 'GLIBC 2.39\nLEGACY 1 1 0 203.0.113.9\nREENTRANT 34 0 -1\n' if bounded else 'GLIBC 2.39\nLEGACY 0 1 0\nREENTRANT 0 0 1\n'
                else:
                    stdout = 'RESULT Err(BufferLimit)\n' if bounded else 'RESULT Ok(None)\n'
                return subprocess.CompletedProcess(command, 0, stdout, '')
            with patch.object(nss, 'ROOT', root), patch.object(nss, 'image_identity', side_effect=[identity, 'sha256:'+'c'*64]), patch.object(nss.subprocess, 'run', side_effect=run) as calls:
                with self.assertRaisesRegex(ValueError, 'binding changed'):
                    nss.qualify(peer, None, self.config())
                self.assertEqual(calls.call_count, 44)
                for call in calls.call_args_list:
                    self.assertIn(identity, call.args[0])
                    self.assertNotIn(self.config()['image'], call.args[0])
            self.assertFalse((root/'artifacts/nss-reference.json').exists())


if __name__ == '__main__':
    unittest.main()
