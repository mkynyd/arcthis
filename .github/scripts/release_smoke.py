"""Validate cargo-dist assets on native runners, without rebuilding binaries."""
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import subprocess
import tarfile
import tempfile


def run(*args):
    return subprocess.check_output(args, text=True)


def main():
    tag = os.environ['RELEASE_TAG']
    target = os.environ['RELEASE_TARGET']
    if not re.fullmatch(r'v\d+\.\d+\.\d+(?:-[a-zA-Z0-9.]+)?', tag):
        raise ValueError('expected an existing version tag')
    expected_machine = 'arm64' if target.startswith('aarch64') else 'x86_64'
    assert platform.machine() == expected_machine, (platform.machine(), target)
    with tempfile.TemporaryDirectory(prefix='arcthis-release-') as temp:
        work = Path(temp)
        asset = f'arcthis-{target}.tar.xz'
        subprocess.run(['gh', 'release', 'download', tag, '--repo', 'mkynyd/arcthis', '--pattern', asset, '--pattern', asset+'.sha256', '--dir', temp], check=True)
        expected = (work/(asset+'.sha256')).read_text().split()[0]
        assert hashlib.sha256((work/asset).read_bytes()).hexdigest() == expected
        with tarfile.open(work/asset) as archive:
            archive.extractall(work, filter='data')
        binaries = list(work.glob('**/arcthis'))
        assert len(binaries) == 1
        binary = str(binaries[0])
        assert run(binary, '--version').strip() == f'arcthis {tag[1:]}'
        assert 'allow-root' in run(binary, 'mcp', '--help')
        if platform.system() == 'Darwin':
            linkage = run('otool', '-L', binary)
            libraries = [line.strip().split(' (', 1)[0] for line in linkage.splitlines()[1:]]
            assert all(name.startswith(('/usr/lib/', '/System/Library/')) for name in libraries), linkage
        else:
            linkage = run('ldd', binary)
            assert 'not found' not in linkage
            allowed = {'linux-vdso.so.1', 'ld-linux-x86-64.so.2', 'libc.so.6', 'libm.so.6', 'libdl.so.2', 'libpthread.so.0', 'librt.so.1', 'libgcc_s.so.1', 'libnettle.so.8', 'liblzma.so.5', 'liblz4.so.1', 'libz.so.1', 'libxml2.so.2'}
            libraries = {Path(line.strip().split()[0]).name for line in linkage.splitlines() if line.strip()}
            def needed(path):
                return set(re.findall(r'\(NEEDED\).*?\[(.*?)\]', run('readelf', '-d', str(path))))
            direct = needed(binary)
            assert direct <= allowed, (direct, linkage)
            resolved = {}
            for line in linkage.splitlines():
                match = re.match(r'\s*(\S+) => (\S+) \(', line)
                if match:
                    resolved[match[1]] = Path(match[2]).resolve()
                elif line.strip().startswith('/'):
                    path = Path(line.strip().split()[0])
                    resolved[path.name] = path.resolve()
            assert all(path.is_file() and str(path).startswith(('/usr/lib/', '/lib/', '/lib64/')) for path in resolved.values()), linkage
            # Ubuntu's system libxml2 pulls ICU, which in turn pulls C++ runtime.
            # Keep direct dependencies strict and prove every additional library
            # is in that actual ELF dependency closure, rather than widening it.
            pending = ['libxml2.so.2'] if 'libxml2.so.2' in resolved else []
            xml_dependencies = set()
            while pending:
                name = pending.pop()
                if name in xml_dependencies:
                    continue
                xml_dependencies.add(name)
                assert name in resolved, (name, linkage)
                pending.extend(needed(resolved[name]))
            assert libraries - allowed <= xml_dependencies, linkage
            print(json.dumps({'direct_dependencies': sorted(direct), 'system_xml_dependency_closure': sorted(xml_dependencies)}))
        print(linkage)
        payload = work/'payload.txt'
        payload.write_bytes('release smoke 内容\n'.encode())
        for suffix in ['zip', 'tar.gz', '7z']:
            packed = work/f'payload.{suffix}'
            packing = json.loads(run(binary, 'pack', str(payload), '--output', str(packed), '--json'))
            assert packing['pack']['verification']['verified']
            listing = json.loads(run(binary, 'list', str(packed), '--json'))
            assert listing['schema_version'] == '2'
            entry = next(item for item in listing['entries'] if item['kind'] == 'file')
            assert subprocess.check_output([binary, 'read', str(packed), entry['path'], '--verify-first']) == payload.read_bytes()
            assert json.loads(run(binary, 'verify', str(packed), '--json'))['verification']['verified']
            assert json.loads(run(binary, 'grep', str(packed), 'release smoke', '--json'))['grep']['files_scanned'] == 1
        for installer in ['arcthis.rb', 'arcthis-npm-package.tar.gz']:
            subprocess.run(['gh', 'release', 'download', tag, '--repo', 'mkynyd/arcthis', '--pattern', installer, '--dir', temp], check=True)
        assert f'/releases/download/{tag}/' in (work/'arcthis.rb').read_text()
        with tarfile.open(work/'arcthis-npm-package.tar.gz') as package:
            members = {member.name: member for member in package.getmembers() if member.isfile()}
            manifests = [member for name, member in members.items() if name.endswith('/package.json') or name == 'package.json']
            assert len(manifests) == 1
            metadata = json.load(package.extractfile(manifests[0]))
            assert metadata['version'] == tag[1:]
            texts = [package.extractfile(member).read() for member in members.values() if member.size < 1024*1024]
            assert any(f'/releases/download/{tag}'.encode() in text for text in texts)
        if platform.system() == 'Darwin' and platform.machine() == 'arm64' and '-' not in tag:
            subprocess.run(['brew', 'install', 'mkynyd/tap/arcthis'], check=True)
            installed = str(Path(run('brew', '--prefix', 'arcthis').strip())/'bin/arcthis')
            assert run(installed, '--version').strip() == f'arcthis {tag[1:]}'
            assert 'allow-root' in run(installed, 'mcp', '--help')
            brew_archive = work/'brew-smoke.zip'
            subprocess.run([installed, 'pack', str(payload), '--output', str(brew_archive)], check=True)
            listing = json.loads(run(installed, 'list', str(brew_archive), '--json'))
            entry = next(item for item in listing['entries'] if item['kind'] == 'file')
            assert subprocess.check_output([installed, 'read', str(brew_archive), entry['path']]) == payload.read_bytes()
            assert json.loads(run(installed, 'verify', str(brew_archive), '--json'))['verification']['verified']
            print('Homebrew clean-runner install: passed')
        print(json.dumps({'tag': tag, 'target': target, 'checksum': 'passed', 'linkage': 'passed', 'formats': 3, 'installers': 'passed'}))


if __name__ == '__main__':
    main()
