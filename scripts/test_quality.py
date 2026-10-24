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
            good = {'data': [{'files': [{'filename': str(path), 'summary': {metric: {'covered': 96, 'count': 100} for metric in ('lines', 'regions')}}]}]}
            self.assertEqual(check(good, manifest, root)['lines']['count'], 100)
            for metric in ('lines', 'regions'):
                for covered, count in [(95, 100), (94, 100), (0, 0)]:
                    bad = copy.deepcopy(good)
                    bad['data'][0]['files'][0]['summary'][metric] = {'covered': covered, 'count': count}
                    with self.assertRaises(ValueError):
                        check(bad, manifest, root)
            with self.assertRaises(ValueError):
                check({'data': [{'files': []}]}, manifest, root)
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
            (root / 'scripts/verify.py').write_text("from pathlib import Path\nassert Path('src/lib.rs').read_text() == 'staged'\nPath('artifacts').mkdir()\nPath('artifacts/proof').write_text('staged')\n")
            subprocess.run(['git', 'init', '--quiet', str(root)], check=True)
            subprocess.run(['git', '-C', str(root), 'add', '.'], check=True)
            (root / 'src/lib.rs').write_text('unstaged')
            with patch.object(verify, 'ROOT', root):
                verify.staged()
            self.assertEqual((root / 'artifacts/staged/proof').read_text(), 'staged')
            self.assertEqual((root / 'src/lib.rs').read_text(), 'unstaged')

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


if __name__ == '__main__':
    unittest.main()
