#!/usr/bin/env python3
# Copyright (C) 2026 Open Source Robotics Foundation
# SPDX-License-Identifier: Apache-2.0
"""Pinned read-only/network-none GNU NSS qualification, with owned child data."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess

ROOT = Path(__file__).resolve().parent.parent


def image_identity(config):
    if config['platform'] != 'linux/amd64' or config['glibc'] != '2.39' or not re.fullmatch(r'ubuntu@sha256:[0-9a-f]{64}', config['image']):
        raise ValueError('Unreviewed NSS reference image/platform')
    metadata = json.loads(subprocess.check_output(['docker', 'image', 'inspect', config['image']]))
    if len(metadata) != 1 or metadata[0]['Os'] != 'linux' or metadata[0]['Architecture'] != 'amd64':
        raise ValueError('Missing exact NSS reference platform')
    identity = metadata[0]['Id']
    if not re.fullmatch(r'sha256:[0-9a-f]{64}', identity):
        raise ValueError('NSS reference image ID is not immutable')
    if not any(value.endswith('@' + config['image'].split('@')[1]) for value in metadata[0].get('RepoDigests', [])):
        raise ValueError('NSS reference image does not match requested digest')
    return identity


def expected_result(case, native):
    lines = native.split('\n')
    if len(lines) != 4 or lines[0] != 'GLIBC 2.39' or lines[-1]:
        raise ValueError('Malformed native GNU NSS observation')
    legacy, reentrant = lines[1].split(), lines[2].split()
    if len(legacy) < 4 or legacy[0] != 'LEGACY' or len(reentrant) < 4 or reentrant[0] != 'REENTRANT':
        raise ValueError('Missing native NSS statuses')
    addresses = legacy[4:]
    if case['expected'] is not None and addresses != case['expected']:
        raise ValueError(f'Native NSS fixture values changed: {case["name"]}')
    if case['bounded']:
        if legacy[1] != '1' or reentrant[1:4] != ['34', '0', '-1']:
            raise ValueError('Missing actual native-success/Rust-scratch-limit boundary')
        return 'Err(BufferLimit)'
    if addresses != reentrant[4:] or (legacy[1] == '1') != (reentrant[1] == '0' and reentrant[2] == '1'):
        raise ValueError('Legacy/reentrant native first-A contract differs')
    if reentrant[1] != '0':
        return f'Err(Lookup {{ code: {reentrant[1]}, host_error: {reentrant[3]} }})'
    return f'Ok(Some({addresses[0]}))' if addresses else 'Ok(None)'


def qualify(executable, profiles, config):
    identity = image_identity(config)  # Before the first child; never run the tag.
    fixture = ROOT / 'tests/fixtures/nss.json'
    cases = json.loads(fixture.read_bytes())['cases']
    if len(cases) != 22 or sum(case['bounded'] for case in cases) != 1:
        raise ValueError('NSS qualification corpus changed without review')
    results = []
    for index, case in enumerate(cases):
        directory = ROOT / 'artifacts/nss-inputs' / str(index)
        directory.mkdir(parents=True, exist_ok=True)
        for name, value in [('hosts', case['hosts']), ('nsswitch.conf', case['nss']), ('host.conf', case['host_conf']),
                            ('resolv.conf', 'nameserver 127.0.0.1\noptions attempts:1 timeout:1\n')]:
            (directory / name).write_text(value)
        common = ['docker', 'run', '--rm', '--network', 'none', '--read-only',
                  '--cap-drop', 'ALL', '--security-opt', 'no-new-privileges',
                  '--user', f'{os.getuid()}:{os.getgid()}',
                  '--mount', f'type=bind,src={ROOT},dst={ROOT},readonly']
        if not executable.is_relative_to(ROOT):
            common += ['--mount', f'type=bind,src={executable.parent},dst={executable.parent},readonly']
        for name in ['hosts', 'nsswitch.conf', 'host.conf', 'resolv.conf']:
            common += ['--mount', f'type=bind,src={directory / name},dst=/etc/{name},readonly']
        native = subprocess.run([*common, identity, str(ROOT / 'artifacts/nss-peer'), case['query']],
                                capture_output=True, text=True, check=True, timeout=10)
        expected = expected_result(case, native.stdout)
        command = list(common)
        if profiles is not None:
            profiles.mkdir(parents=True, exist_ok=True)
            # Container PID namespaces reuse PID 1; each case needs its own name.
            command += ['--mount', f'type=bind,src={profiles},dst={profiles}',
                        '--env', f'LLVM_PROFILE_FILE={profiles}/nss-{index}-%m-%p.profraw']
        command += ['--env', f'GZ_NSS_CHILD_NAMES={case["query"]}', identity, str(executable),
                    '--exact', 'lookup_child', '--nocapture']
        rust = subprocess.run(command, capture_output=True, text=True, check=True, timeout=10)
        values = [line.split('RESULT ', 1)[1] for line in rust.stdout.split('\n') if 'RESULT ' in line]
        if values != [expected] or native.stderr or rust.stderr:
            raise ValueError(f'NSS owned boundary differs: {case["name"]}: {native!r} {rust!r}')
        results.append(dict(name=case['name'], native_stdout=native.stdout, rust_stdout=rust.stdout,
                            expected=expected, native_stderr=native.stderr, rust_stderr=rust.stderr))
    after = image_identity(config)
    if identity != after:
        raise ValueError('NSS image digest binding changed during qualification')
    report = dict(image=config['image'], invoked_image=identity, image_before=identity, image_after=after,
                  fixture_sha256=hashlib.sha256(fixture.read_bytes()).hexdigest(),
                  native_sha256=hashlib.sha256((ROOT / 'artifacts/nss-peer').read_bytes()).hexdigest(),
                  executable_sha256=hashlib.sha256(executable.read_bytes()).hexdigest(), cases=results)
    (ROOT / 'artifacts/nss-reference.json').write_text(json.dumps(report, indent=2) + '\n')
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--prepare', action='store_true')
    parser.add_argument('--test-executable', type=Path)
    parser.add_argument('--profiles', type=Path)
    args = parser.parse_args()
    config = json.loads((ROOT / 'docs/nss-reference.json').read_bytes())
    if args.prepare:
        try:
            identity = image_identity(config)
        except subprocess.CalledProcessError:
            subprocess.run(['docker', 'pull', '--platform', config['platform'], config['image']], check=True)
            identity = image_identity(config)
        print('Prepared immutable NSS reference', identity)
    if args.test_executable is not None:
        report = qualify(args.test_executable.resolve(), args.profiles.resolve() if args.profiles else None, config)
        print('Actual first-A NSS cases passed:', len(report['cases']))
    if not args.prepare and args.test_executable is None:
        parser.error('select --prepare or --test-executable')


if __name__ == '__main__':
    main()
