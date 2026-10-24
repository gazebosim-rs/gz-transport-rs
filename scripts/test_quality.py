# Copyright (C) 2026 Open Source Robotics Foundation
# SPDX-License-Identifier: Apache-2.0
import copy
import tempfile
import unittest
from pathlib import Path
from coverage import above_95, check, changed_check


class CoveragePolicy(unittest.TestCase):
    def test_strict_integer_boundary(self):
        for covered, count in [(0, 0), (95, 100), (9499, 10000), (100, 0), (101, 100)]:
            self.assertFalse(above_95(covered, count))
        self.assertTrue(above_95(9501, 10000))

    def test_missing_empty_and_below_threshold(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / 'src').mkdir()
            path = root / 'src/lib.rs'
            path.write_text('pub fn f() {}')
            manifest = {'production_files': ['src/lib.rs'], 'production_roots': ['src'], 'exclusions': [], 'crate': 'test', 'profile': 'test'}
            good = {'type':'llvm.coverage.json.export', 'data': [{'files': [{'filename': str(path), 'summary': {metric: {'covered': 96, 'count': 100} for metric in ('lines', 'regions')}}]}]}
            self.assertEqual(check(good, manifest, root)['lines']['count'], 100)
            for metric in ('lines', 'regions'):
                for covered, count in [(95, 100), (94, 100), (0, 0)]:
                    bad = copy.deepcopy(good)
                    bad['data'][0]['files'][0]['summary'][metric] = {'covered': covered, 'count': count}
                    with self.assertRaises(ValueError):
                        check(bad, manifest, root)
            with self.assertRaises(ValueError):
                check({'data': [{'files': []}]}, manifest, root)
            for invalid in ['type', 'duplicate', 'outside']:
                bad = copy.deepcopy(good)
                if invalid == 'type':
                    bad['type'] = 'missing'
                elif invalid == 'duplicate':
                    bad['data'][0]['files'] *= 2
                else:
                    bad['data'][0]['files'].append(dict(good['data'][0]['files'][0], filename=str(root/'production.inc')))
                with self.subTest(invalid=invalid), self.assertRaises(ValueError):
                    check(bad, manifest, root)
            (root / 'src/new.rs').write_text('pub fn uncovered() {}')
            with self.assertRaises(ValueError):
                check(good, manifest, root)

    def test_changed_lines(self):
        root = Path('/tmp')
        with self.assertRaises(ValueError):
            changed_check('SF:/tmp/lib.rs\nDA:1,0', {'lib.rs': {1}}, root)
        self.assertEqual(changed_check('SF:/tmp/lib.rs\nDA:1,1', {'lib.rs': {1}}, root)['covered'], 1)


class StagedAndEvidencePolicy(unittest.TestCase):
    def test_exact_staged_snapshot(self):
        import subprocess
        import verify
        from unittest.mock import patch
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory) / 'repo'
            (root / 'scripts').mkdir(parents=True)
            (root / 'src').mkdir()
            (root / 'src/lib.rs').write_text('staged')
            (root / '.gitattributes').write_text('src/lib.rs export-ignore\n')
            (root / 'scripts/verify.py').write_text("from pathlib import Path\nassert Path('src/lib.rs').read_text() == 'staged'\nPath('artifacts').mkdir()\nPath('artifacts/proof').write_text('staged')\n")
            subprocess.run(['git', 'init', '--quiet', str(root)], check=True)
            subprocess.run(['git', '-C', str(root), 'add', '.'], check=True)
            (root / 'src/lib.rs').write_text('unstaged')
            with patch.object(verify, 'ROOT', root):
                verify.staged()
            self.assertEqual((root / 'artifacts/staged/proof').read_text(), 'staged')
            self.assertEqual((root / 'src/lib.rs').read_text(), 'unstaged')

    def test_checkout_attributes_cannot_substitute_transformed_staged_source(self):
        import subprocess
        import verify
        from unittest.mock import patch
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory) / 'repo'
            (root / 'src').mkdir(parents=True)
            (root / 'src/lib.rs').write_bytes(b'pub fn staged() {}\n')
            (root / '.gitattributes').write_text('src/lib.rs text eol=crlf\n')
            subprocess.run(['git', 'init', '--quiet', str(root)], check=True)
            subprocess.run(['git', '-C', str(root), 'add', '.'], check=True, capture_output=True)
            with patch.object(verify, 'ROOT', root), self.assertRaisesRegex(ValueError, 'Exported staged blob differs'):
                verify.staged()

    def test_snapshot_identity_checks_content_file_type_and_executable_mode(self):
        import shutil
        import subprocess
        import verify
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory) / 'repo'
            root.mkdir()
            source = root / 'source'
            source.write_bytes(b'original')
            subprocess.run(['git', 'init', '--quiet', str(root)], check=True)
            subprocess.run(['git', '-C', str(root), 'add', '.'], check=True)
            tree = verify.staged_tree(root)
            candidate = Path(directory) / 'candidate'
            candidate.mkdir()
            exported = candidate / 'source'
            shutil.copy2(source, exported)
            verify.validate_snapshot(root, tree, candidate)
            extra = candidate / 'untracked.rs'
            extra.write_text('untracked source')
            with self.assertRaisesRegex(ValueError, 'paths differ'):
                verify.validate_snapshot(root, tree, candidate)
            extra.unlink()
            exported.write_bytes(b'substituted')
            with self.assertRaisesRegex(ValueError, 'blob differs'):
                verify.validate_snapshot(root, tree, candidate)
            exported.write_bytes(b'original')
            exported.chmod(0o755)
            with self.assertRaisesRegex(ValueError, 'executable mode differs'):
                verify.validate_snapshot(root, tree, candidate)
            source.chmod(0o755)
            subprocess.run(['git', '-C', str(root), 'add', 'source'], check=True)
            executable_tree = verify.staged_tree(root)
            exported.chmod(0o645)  # Other-execute cannot replace Git's owner-execute bit.
            with self.assertRaisesRegex(ValueError, 'executable mode differs'):
                verify.validate_snapshot(root, executable_tree, candidate)
            exported.unlink()
            exported.symlink_to(source)
            with self.assertRaisesRegex(ValueError, 'file type differs'):
                verify.validate_snapshot(root, tree, candidate)

    def test_concurrent_index_or_head_edit_cannot_validate_an_unchecked_commit(self):
        import subprocess
        import verify
        from unittest.mock import patch
        for mutation in ('index', 'head'):
            with self.subTest(mutation=mutation), tempfile.TemporaryDirectory() as directory:
                root = Path(directory) / 'repo'
                (root / 'scripts').mkdir(parents=True)
                (root / 'src').mkdir()
                (root / 'src/lib.rs').write_text('staged')
                mutate = (
                    "(root/'src/lib.rs').write_text('unchecked concurrent edit')\nsubprocess.run(['git','-C',str(root),'add','src/lib.rs'],check=True)"
                    if mutation == 'index' else
                    "subprocess.run(['git','-C',str(root),'-c','user.name=Gate test','-c','user.email=gate@example.invalid','commit','--quiet','-m','concurrent HEAD change'],check=True)"
                )
                script = f"""from pathlib import Path
import subprocess
assert Path('src/lib.rs').read_text() == 'staged'
root=Path({str(root)!r})
{mutate}
Path('artifacts').mkdir()
Path('artifacts/proof').write_text('old tree passed')
"""
                (root / 'scripts/verify.py').write_text(script)
                subprocess.run(['git', 'init', '--quiet', str(root)], check=True)
                subprocess.run(['git', '-C', str(root), 'add', '.'], check=True)
                with patch.object(verify, 'ROOT', root), self.assertRaisesRegex(ValueError, 'Git index or HEAD changed'):
                    verify.staged()
                self.assertFalse((root / 'artifacts/staged/proof').exists())

    def test_stale_source_or_profile_is_rejected(self):
        import hashlib
        from verify import validate_evidence
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / 'source.rs').write_text('before')
            evidence = {'profile': 'native', 'inputs': {'source.rs': hashlib.sha256(b'before').hexdigest()}}
            validate_evidence(evidence, root, 'native')
            with self.assertRaises(ValueError):
                validate_evidence(evidence, root, 'missing-profile')
            (root / 'source.rs').write_text('after')
            with self.assertRaises(ValueError):
                validate_evidence(evidence, root, 'native')


class ChangedSourcePolicy(unittest.TestCase):
    def test_changed_line_ranges_include_new_files(self):
        from verify import changed_lines
        diff='+++ b/src/lib.rs\n@@ -2 +2,2 @@\n+++ b/src/config.rs\n@@ -0,0 +1,3 @@\n+++ b/src/deletion.rs\n@@ -4,2 +4,0 @@\n'
        self.assertEqual(changed_lines(diff),{'src/lib.rs':{2,3},'src/config.rs':{1,2,3},'src/deletion.rs':set()})


class WorkspaceInstrumentationPolicy(unittest.TestCase):
    def test_wrapper_preserves_arguments_and_adds_instrumentation(self):
        import subprocess
        wrapper = Path(__file__).resolve().parent / 'rustc-coverage.py'
        result = subprocess.check_output([str(wrapper), '/bin/echo', '--crate-name', 'owned'], text=True)
        self.assertEqual(result.strip(), '--crate-name owned -C instrument-coverage')


class LcovOwnershipPolicy(unittest.TestCase):
    def test_missing_empty_or_duplicate_lcov_is_not_a_pass(self):
        root=Path('/tmp')
        for text in ['', 'SF:/tmp/a.rs\n', 'SF:/tmp/a.rs\nSF:/tmp/a.rs\n', 'SF:/tmp/a.rs\nDA:1,0\nDA:1,1\n', 'DA:1,1\n', 'SF:/tmp/a.rs\nDA:0,1\n', 'SF:/tmp/a.rs\nDA:1,-1\n']:
            with self.subTest(text=text),self.assertRaises(ValueError):
                changed_check(text, {'a.rs':{1}}, root, ['a.rs'])


class PushRangePolicy(unittest.TestCase):
    def test_multiple_commits_and_untracked_sources_are_in_changed_range(self):
        import subprocess
        from verify import source_changes
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory)
            (root/'src').mkdir()
            git=lambda *args:subprocess.check_output(['git','-C',str(root),*args],text=True).strip()
            git('init','--quiet')
            path=root/'src/lib.rs'
            for index in range(1,4):
                path.write_text(''.join(f'pub fn f{number}() {{}}\n' for number in range(1,index+1)))
                git('add','.')
                git('-c','user.name=Gate test','-c','user.email=gate@example.invalid','commit','--quiet','-m','fixture')
                if index==1:base=git('rev-parse','HEAD')
            self.assertEqual(source_changes(root,['src/lib.rs'],base),{'src/lib.rs':{2,3}})
            (root/'src/new.rs').write_text('pub fn added() {}\n')
            self.assertEqual(source_changes(root,['src/lib.rs','src/new.rs'],base),{'src/lib.rs':{2,3},'src/new.rs':{1}})
            self.assertEqual(source_changes(root,['src/lib.rs'],'0'*40),{'src/lib.rs':{1,2,3}})
            with self.assertRaises(ValueError):source_changes(root,['src/lib.rs'],'HEAD~2')


class CargoLayoutPolicy(unittest.TestCase):
    def manifest(self):
        return {'crate':'gz-transport','production_roots':['src'],'exclusions':[], 'required_runners':['linux-x86_64-multicast','pinned-cpp-peer']}

    def test_real_cargo_implicit_targets_and_features_are_rejected_before_build(self):
        import json
        import subprocess
        from verify import check_layout
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory)
            (root/'src').mkdir()
            (root/'src/lib.rs').write_text('pub fn function() {}')
            cargo='[package]\nname="gz-transport"\nversion="0.1.0"\nedition="2024"\nlicense="Apache-2.0"\n'
            (root/'Cargo.toml').write_text(cargo)
            def metadata():
                return json.loads(subprocess.check_output(['cargo','metadata','--offline','--no-deps','--format-version=1'],cwd=root,text=True))
            check_layout(root,metadata(),self.manifest())
            for relative in ['build.rs','src/main.rs','src/bin/tool.rs','examples/example.rs','benches/bench.rs']:
                path=root/relative
                path.parent.mkdir(parents=True,exist_ok=True)
                path.write_text('compile_error!("must not compile before layout rejection");')
                with self.subTest(target=relative),self.assertRaisesRegex(ValueError,'Unaccounted Cargo target'):
                    check_layout(root,metadata(),self.manifest())
                path.unlink()
            (root/'Cargo.toml').write_text(cargo+'\n[features]\nextra=[]\n')
            with self.assertRaisesRegex(ValueError,'Feature profiles'):
                check_layout(root,metadata(),self.manifest())

    def test_packages_sources_license_and_runner_require_supported_profiles(self):
        from verify import check_layout
        from unittest.mock import patch
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory)
            (root/'src').mkdir()
            (root/'src/lib.rs').write_text('pub fn function() {}')
            package={'id':'owned','name':'gz-transport','source':None,'manifest_path':str(root/'Cargo.toml'),'license':'Apache-2.0','features':{},'targets':[{'kind':['lib'],'crate_types':['lib'],'src_path':str(root/'src/lib.rs')}]}
            metadata={'packages':[package],'workspace_members':['owned']}
            check_layout(root,metadata,self.manifest())
            for key,value in [('packages',[package,dict(package,id='other')]),('workspace_members',['owned','other'])]:
                with self.subTest(metadata=key),self.assertRaises(ValueError):
                    check_layout(root,dict(metadata,**{key:value}),self.manifest())
            for key,value in [('license','MIT'),('targets',[]),('targets',[dict(package['targets'][0],src_path=str(root/'outside.rs'))])]:
                with self.subTest(package=key),self.assertRaises(ValueError):
                    check_layout(root,dict(metadata,packages=[dict(package,**{key:value})]),self.manifest())
            with patch('verify.platform.system',return_value='Windows'),self.assertRaisesRegex(ValueError,'runner unavailable'):
                check_layout(root,metadata,self.manifest())
            (root/'forgotten.rs').write_text('pub fn omitted() {}')
            with self.assertRaisesRegex(ValueError,'Unaccounted owned Rust source'):
                check_layout(root,metadata,self.manifest())


class CompilerInputPolicy(unittest.TestCase):
    def test_normal_library_cannot_import_production_code_from_test_or_data_paths(self):
        import json
        import os
        import subprocess
        from verify import check_layout, production_inputs
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory)/'repo with spaces'
            (root/'src').mkdir(parents=True)
            (root/'tests').mkdir()
            (root/'Cargo.toml').write_text('[package]\nname="gz-transport"\nversion="0.1.0"\nedition="2024"\nlicense="Apache-2.0"\n')
            source=root/'src/lib.rs'
            source.write_text('pub fn root() {}\n#[cfg(test)] #[path="../tests/support.rs"] mod support;')
            (root/'tests/support.rs').write_text('compile_error!("cfg(test) source must not enter normal library audit");')
            env=dict(os.environ,RUSTUP_TOOLCHAIN='1.94.1',CARGO_TARGET_DIR=str(root/'target'))
            subprocess.run(['cargo','generate-lockfile','--offline'],cwd=root,env=env,check=True)
            def metadata():
                return json.loads(subprocess.check_output(['cargo','metadata','--offline','--no-deps','--format-version=1'],cwd=root,env=env,text=True))
            layout=CargoLayoutPolicy().manifest()
            package=check_layout(root,metadata(),layout)
            proof=production_inputs(root,['src/lib.rs'],package['id'],env,root/'artifacts')
            self.assertEqual(proof['inputs'],['src/lib.rs'])
            for name in ['tests/hidden.rs','tests/hidden.inc','outside.inc']:
                path=root/name
                path.write_text('pub fn hidden() {}')
                module = f'#[path="../{name}"] pub mod hidden;' if name.endswith('.rs') else f'include!("../{name}");'
                stamp=source.stat()
                source.write_text(f'pub fn root() {{}}\n{module}')
                os.utime(source,ns=(stamp.st_atime_ns,stamp.st_mtime_ns))
                package=check_layout(root,metadata(),layout)
                with self.subTest(name=name),self.assertRaisesRegex(ValueError,'Normal library compiler inputs differ'):
                    production_inputs(root,['src/lib.rs'],package['id'],env,root/'artifacts')
                self.assertFalse((root/'artifacts/production-inputs.json').exists())
                path.unlink()

    def test_depinfo_missing_duplicate_or_escaped_inputs_fail_closed(self):
        from verify import depinfo_inputs
        root=Path('/tmp')
        good='output.rlib: src/lib.rs\n\nsrc/lib.rs:\n'
        self.assertEqual(depinfo_inputs(good,root,['src/lib.rs']),['src/lib.rs'])
        for bad in ['',good+'src/lib.rs:\n',good.replace('src/lib.rs','src/hidden.rs'), 'output.rlib: src/lib.rs\n',good.replace('src/lib.rs','src/with\\ space.rs')]:
            with self.subTest(bad=bad),self.assertRaises(ValueError):
                depinfo_inputs(bad,root,['src/lib.rs'])


if __name__ == '__main__':
    unittest.main()
