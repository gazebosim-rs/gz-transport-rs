"""Reconcile applicable reference configurations without retiring baseline cases."""
from pathlib import Path
import hashlib
import re


def enabled_features(reference: Path):
    header = (reference / 'build/include/gz/transport/config.hh').read_text()
    return {'zeromq', 'zenoh'} if re.search(r'^#define HAVE_ZENOH\s+1$', header, re.M) else {'zeromq'}


def check_ledger(ledger, expanded, features, catalog):
    if ledger['upstream_sha'] != catalog['upstream_sha'] or expanded['upstream_sha'] != catalog['upstream_sha']:
        raise ValueError('Reference catalog revision mismatch')
    entries = ledger['entries']
    ids = [entry['id'] for entry in entries]
    if len(ids) != catalog['case_count'] or hashlib.sha256('\n'.join(sorted(ids)).encode()).hexdigest() != catalog['case_ids_sha256']:
        raise ValueError('Full retained catalog changed; baseline review required')
    if len(ids) != len(set(ids)):
        raise ValueError('Duplicate upstream test ID')
    for entry in entries:
        if set(entry.get('reference_features', [])) - {'zenoh'}:
            raise ValueError('Unknown reference feature condition')
    applicable = {entry['id'] for entry in entries if set(entry.get('reference_features', [])) <= features}
    actual = {entry['id'] for entry in expanded['entries']}
    if applicable != actual or expanded['unavailable_targets'] or ledger['source_fixture_sha256'] != expanded['source_fixture_sha256']:
        raise ValueError(f'Reference ledger mismatch: missing={sorted(applicable-actual)}, extra={sorted(actual-applicable)}')
    for entry in entries:
        if entry['disposition'] != 'pending' and (not entry['replacement'] or not entry.get('evidence')):
            raise ValueError('Missing retained-test mapping/evidence')
    return {'features': sorted(features), 'applicable_cases': len(applicable), 'feature_conditioned_cases': len(ids)-len(applicable), 'retained_cases': len(ids)}


def check_mapped_tests(ledger, output):
    passed = set(re.findall(r'^test (\S+) \.\.\. ok$', output, re.M))
    for entry in ledger['entries']:
        for replacement in entry['replacement']:
            if replacement not in passed:
                raise ValueError(f'Mapped retained assertion did not execute and pass: {replacement}')
