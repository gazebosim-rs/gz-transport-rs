# Copyright (C) 2026 Open Source Robotics Foundation
# SPDX-License-Identifier: Apache-2.0
import copy
import hashlib
from pathlib import Path
import tempfile
import unittest
from reference import check_ledger, enabled_features, check_mapped_tests


class ReferenceConfigurations(unittest.TestCase):
    def ledger(self):
        return {'upstream_sha':'pinned', 'entries':[{'id':'core', 'reference_features':[], 'disposition':'pending'}, {'id':'optional', 'reference_features':['zenoh'], 'disposition':'pending'}], 'source_fixture_sha256':{'source':'hash'}}

    def test_feature_expansion_retains_missing_optional_cases(self):
        ledger = self.ledger()
        core = {'upstream_sha':'pinned', 'entries':[{'id':'core'}], 'source_fixture_sha256':{'source':'hash'}, 'unavailable_targets':[]}
        catalog = {'upstream_sha':'pinned', 'case_count':2, 'case_ids_sha256':hashlib.sha256(b'core\noptional').hexdigest()}
        result = check_ledger(ledger, core, {'zeromq'},catalog)
        self.assertEqual(result['retained_cases'], 2)
        self.assertEqual(result['feature_conditioned_cases'], 1)
        with self.assertRaises(ValueError):
            check_ledger(ledger, core, {'zeromq', 'zenoh'},catalog)
        for bad in [dict(core, entries=[]), dict(core, unavailable_targets=['runner failed']), dict(core, source_fixture_sha256={})]:
            with self.assertRaises(ValueError):
                check_ledger(ledger, bad, {'zeromq'},catalog)
        missing_optional = copy.deepcopy(ledger)
        missing_optional['entries'].pop()
        with self.assertRaises(ValueError):
            check_ledger(missing_optional, core, {'zeromq'},catalog)
        duplicate = copy.deepcopy(ledger)
        duplicate['entries'].append(duplicate['entries'][0])
        with self.assertRaises(ValueError):
            check_ledger(duplicate, core, {'zeromq'},catalog)

    def test_feature_detection_requires_built_header(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            with self.assertRaises(FileNotFoundError):
                enabled_features(root)
            header = root / 'build/include/gz/transport/config.hh'
            header.parent.mkdir(parents=True)
            header.write_text('/* #undef HAVE_ZENOH */\n')
            self.assertEqual(enabled_features(root), {'zeromq'})
            header.write_text('#define HAVE_ZENOH 1\n')
            self.assertEqual(enabled_features(root), {'zeromq', 'zenoh'})


class RetainedMappings(unittest.TestCase):
    def test_removed_or_ignored_mapping_is_not_a_pass(self):
        ledger={'entries':[{'replacement':['assertions::ported']}]}
        check_mapped_tests(ledger,'test assertions::ported ... ok\n')
        for output in ['', 'test assertions::ported ... ignored\n', 'test assertions::ported ... FAILED\n']:
            with self.assertRaises(ValueError):
                check_mapped_tests(ledger,output)


class RuntimeEnumeration(unittest.TestCase):
    def test_structured_listing_counts_and_duplicate_ids_are_checked(self):
        from inventory import listed_cases
        report={'tests':2,'testsuites':[{'name':'Suite','tests':2,'testsuite':[{'name':'First'},{'name':'Second/0'}]}]}
        self.assertEqual(listed_cases(report),['Suite.First','Suite.Second/0'])
        for bad in [dict(report,tests=3),{'tests':2,'testsuites':[{'name':'Suite','tests':3,'testsuite':[{'name':'First'},{'name':'First'}]}]}, {'tests':2,'testsuites':[{'name':'Suite','tests':2,'testsuite':[{'name':'First'},{'name':'First'}]}]}]:
            with self.assertRaises(ValueError):listed_cases(bad)


    def test_native_environment_forwarding_and_loader_failure_are_retained(self):
        import json
        from types import SimpleNamespace
        from unittest.mock import patch
        from inventory import collect
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory)
            (root/'build/bin').mkdir(parents=True)
            (root/'build/bin/UNIT_peer').touch()
            diagnostics=root/'diagnostics'
            native_env={'LD_LIBRARY_PATH':'/isolated/reference/lib'}
            report={'tests':1,'testsuites':[{'name':'Suite','tests':1,'testsuite':[{'name':'Case'}]}]}
            def enumerate_peer(command, **kwargs):
                self.assertEqual(kwargs['env'],native_env)
                self.assertIn('--gtest_filter=*',command)
                listing=Path(next(arg.removeprefix('--gtest_output=json:') for arg in command if arg.startswith('--gtest_output=json:')))
                listing.write_text(json.dumps(report))
                return SimpleNamespace(returncode=0,stdout='main banner',stderr='')
            with patch('inventory.subprocess.check_output',return_value='pinned'),patch('inventory.subprocess.run',side_effect=enumerate_peer):
                expanded=collect(root,diagnostics,env=native_env)
            self.assertEqual([e['case'] for e in expanded['entries']],['Suite.Case'])
            self.assertEqual((diagnostics/'UNIT_peer.stdout').read_text(),'main banner')
            failure=SimpleNamespace(returncode=127,stdout='',stderr='libgz-utils.so.5: cannot open shared object file')
            with patch('inventory.subprocess.check_output',return_value='pinned'),patch('inventory.subprocess.run',return_value=failure):
                failed=collect(root,diagnostics,env=native_env)
            self.assertEqual(failed['entries'],[])
            self.assertFalse((diagnostics/'UNIT_peer.json').exists())
            self.assertIn('libgz-utils.so.5',failed['unavailable_targets'][0]['stderr'])
            self.assertEqual((diagnostics/'UNIT_peer.stderr').read_text(),failure.stderr)
