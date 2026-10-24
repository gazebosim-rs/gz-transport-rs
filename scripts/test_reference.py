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
