# Copyright (C) 2026 Open Source Robotics Foundation
# SPDX-License-Identifier: Apache-2.0
"""The small FFI crate must not hide behind aggregate Transport coverage."""
import copy
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

from coverage import check, changed_check, owned_crates
from verify import check_layout, production_inputs, source_changes


class OwnedCratePolicy(unittest.TestCase):
    def manifest(self):
        return dict(crate='gz-transport', profile='test',
                    production_roots=['src', 'nss/src'],
                    production_files=['src/lib.rs', 'nss/src/lib.rs'],
                    test_roots=['tests', 'nss/tests'], exclusions=[],
                    required_runners=['linux-x86_64-multicast', 'pinned-cpp-peer', 'glibc-2.39-network-none'],
                    owned_crates={'gz-transport': ['src/lib.rs'], 'gz-transport-nss': ['nss/src/lib.rs']})

    def test_small_owned_crate_exactly_95_cannot_hide_in_large_covered_total(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            manifest = self.manifest()
            files = []
            for name, count in [('src/lib.rs', 10000), ('nss/src/lib.rs', 20)]:
                path = root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text('pub fn owned() {}\n')
                files.append(dict(filename=str(path), summary={metric: dict(covered=count, count=count) for metric in ['lines', 'regions']}))
            good = {'type': 'llvm.coverage.json.export', 'data': [{'files': files}]}
            self.assertEqual(check(good, manifest, root)['crates']['gz-transport-nss']['lines'], {'covered': 20, 'count': 20})
            for metric in ['lines', 'regions']:
                bad = copy.deepcopy(good)
                bad['data'][0]['files'][1]['summary'][metric]['covered'] = 19
                with self.subTest(metric=metric), self.assertRaisesRegex(ValueError, 'gz-transport-nss'):
                    check(bad, manifest, root)
            for groups in [
                {'gz-transport': ['src/lib.rs']},
                {'gz-transport': ['src/lib.rs', 'nss/src/lib.rs'], 'gz-transport-nss': ['nss/src/lib.rs']},
                {'gz-transport': ['src/lib.rs'], 'gz-transport-nss': []},
            ]:
                with self.subTest(groups=groups), self.assertRaises(ValueError):
                    owned_crates(dict(manifest, owned_crates=groups))

    def test_changed_ffi_lines_fail_even_when_aggregate_exceeds_threshold(self):
        root = Path('/tmp/owned-changed')
        lcov = f'SF:{root}/src/lib.rs\n' + ''.join(f'DA:{line},1\n' for line in range(1, 1001))
        lcov += f'SF:{root}/nss/src/lib.rs\nDA:1,0\n'
        changed = {'src/lib.rs': set(range(1, 1001)), 'nss/src/lib.rs': {1}}
        self.assertEqual(changed_check(lcov, changed, root)['covered'], 1000)
        with self.assertRaisesRegex(ValueError, 'Changed executable'):
            changed_check(lcov, {'nss/src/lib.rs': {1}}, root, ['nss/src/lib.rs'])

    def test_real_workspace_sources_are_owned_and_normal_ffi_imports_cannot_hide(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for name in ['src', 'nss/src', 'nss/tests']:
                (root / name).mkdir(parents=True, exist_ok=True)
            (root / 'Cargo.toml').write_text('[package]\nname="gz-transport"\nversion="0.1.0"\nedition="2024"\nlicense="Apache-2.0"\n[workspace]\nmembers=["nss"]\n')
            (root / 'nss/Cargo.toml').write_text('[package]\nname="gz-transport-nss"\nversion="0.1.0"\nedition="2024"\nlicense="Apache-2.0"\n')
            (root / 'src/lib.rs').write_text('pub fn parent() {}\n')
            (root / 'nss/src/lib.rs').write_text('pub fn boundary() {}\n')
            env = dict(os.environ, RUSTUP_TOOLCHAIN='1.94.1', CARGO_TARGET_DIR=str(root / 'target'))
            subprocess.run(['cargo', 'generate-lockfile', '--offline'], cwd=root, env=env, check=True, capture_output=True)
            metadata = json.loads(subprocess.check_output(['cargo', 'metadata', '--offline', '--no-deps', '--format-version=1'], cwd=root, env=env))
            check_layout(root, metadata, self.manifest())
            package = next(package for package in metadata['packages'] if package['name'] == 'gz-transport-nss')
            proof = production_inputs(root, ['nss/src/lib.rs'], package['id'], env, root / 'artifacts', 'gz-transport-nss')
            self.assertEqual(proof['inputs'], ['nss/src/lib.rs'])
            bad = copy.deepcopy(metadata)
            bad['packages'].append(dict(package, id='hidden', name='hidden'))
            with self.assertRaisesRegex(ValueError, 'Additional local'):
                check_layout(root, bad, self.manifest())
            (root / 'nss/tests/hidden.rs').write_text('pub fn hidden() {}\n')
            (root / 'nss/src/lib.rs').write_text('include!("../tests/hidden.rs");\n')
            with self.assertRaisesRegex(ValueError, 'Normal library compiler inputs differ'):
                production_inputs(root, ['nss/src/lib.rs'], package['id'], env, root / 'artifacts', 'gz-transport-nss')
            self.assertFalse((root / 'artifacts/gz-transport-nss-production-inputs.json').exists())

    def test_real_git_diff_includes_tracked_nss_source_hunks(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / 'nss/src').mkdir(parents=True)
            path = root / 'nss/src/lib.rs'
            path.write_text('pub fn initial() {}\n')
            subprocess.run(['git', 'init', '--quiet', str(root)], check=True)
            subprocess.run(['git', 'add', '.'], cwd=root, check=True)
            subprocess.run(['git', '-c', 'user.name=Gate test', '-c', 'user.email=test@example.invalid', 'commit', '--quiet', '-m', 'base'], cwd=root, check=True)
            path.write_text('pub fn changed() {}\n')
            self.assertEqual(source_changes(root, ['nss/src/lib.rs']), {'nss/src/lib.rs': {1}})


if __name__ == '__main__':
    unittest.main()
