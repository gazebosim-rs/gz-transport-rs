#!/usr/bin/env python3
"""Build an isolated immutable native reference on an ordinary Ubuntu host."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parent.parent


def run(command, log, env):
    print(' '.join(map(str, command)), flush=True)
    with log.open('a') as output:
        result = subprocess.run(list(map(str, command)), stdout=output, stderr=subprocess.STDOUT, env=env)
    if result.returncode:
        print('\n'.join(log.read_text(errors='replace').splitlines()[-70:]), flush=True)
        raise subprocess.CalledProcessError(result.returncode, command)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--root', type=Path, default=ROOT / 'artifacts/reference')
    parser.add_argument('--local-sources', type=Path)
    parser.add_argument('--jobs', type=int, default=2)
    args = parser.parse_args()
    if args.jobs < 1:
        parser.error('--jobs must be positive')
    reference = args.root.resolve()
    prefix = reference / 'install'
    logs = reference / 'logs'
    logs.mkdir(parents=True, exist_ok=True)
    lock = json.loads((ROOT / 'docs/reference-lock.json').read_text())
    env = dict(os.environ, CMAKE_PREFIX_PATH=str(prefix), PKG_CONFIG_PATH=str(prefix / 'lib/pkgconfig'), LD_LIBRARY_PATH=str(prefix / 'lib'))
    env['PATH'] = f'{prefix}/bin:{env["PATH"]}'
    for repository in lock['repositories']:
        name, sha = repository['name'], repository['sha']
        source = reference / 'sources' / name
        log = logs / f'{name}.log'
        if not (source / '.git').exists():
            source.mkdir(parents=True, exist_ok=True)
            run(['git', 'init', '--quiet', source], log, env)
            origin = str(args.local_sources.resolve() / name) if args.local_sources else f'https://github.com/gazebosim/{name}'
            run(['git', '-C', source, 'fetch', '--depth', '1', origin, sha], log, env)
            run(['git', '-C', source, 'checkout', '--detach', 'FETCH_HEAD'], log, env)
        actual = subprocess.check_output(['git', '-C', str(source), 'rev-parse', 'HEAD'], env=env, text=True).strip()
        if actual != sha:
            raise ValueError(f'Unexpected source revision: {name}')
        subprocess.run(['git', '-C', str(source), 'diff', '--exit-code', 'HEAD'], env=env, check=True)
        build = source / 'build'
        configure = ['cmake', '-S', source, '-B', build, '-G', 'Unix Makefiles', f'-DCMAKE_INSTALL_PREFIX={prefix}', '-DCMAKE_INSTALL_LIBDIR=lib', '-DCMAKE_BUILD_TYPE=Release', f'-DBUILD_TESTING={"ON" if name == "gz-transport" else "OFF"}', '-DSKIP_PYBIND11=ON', '-DSKIP_SWIG=ON', '-DGZ_UTILS_VENDOR_CLI11=ON']
        if name == 'gz-transport':
            configure.extend(['-DGZ_TRANSPORT_ENABLE_ZENOH=OFF', '-DGZ_TRANSPORT_DEFAULT_IMPLEMENTATION=zeromq'])
        run(configure, log, env)
        run(['cmake', '--build', build, '--parallel', str(args.jobs)], log, env)
        run(['cmake', '--install', build], log, env)
    binaries = {str(p.relative_to(prefix)): hashlib.sha256(p.read_bytes()).hexdigest() for p in (prefix / 'lib').glob('libgz*.so*') if p.is_file()}
    metadata = {'source_lock': lock, 'compiler': subprocess.check_output(['c++', '--version'], env=env, text=True), 'cmake': subprocess.check_output(['cmake', '--version'], env=env, text=True), 'packages': subprocess.check_output(['dpkg-query', '-W', *lock['apt_dependencies']], env=env, text=True), 'libraries_sha256': binaries}
    (reference / 'environment.json').write_text(json.dumps(metadata, indent=2) + '\n')
    print(f'GZ_REFERENCE_SOURCE={reference}/sources/gz-transport\nGZ_REFERENCE_PREFIX={prefix}', flush=True)


if __name__ == '__main__':
    main()
