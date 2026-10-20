#!/usr/bin/env python3
"""Apply reviewed per-project manual hooks and reject incomplete integrations.

Matches Baka-SU/BakaSU kernel/tools/manual_hook_check.mk, plus the exec post
hook required to report failed executions. Run before setup/configuration, and
again with --check-config after olddefconfig. No upstream checks are bypassed.
"""
import argparse
from pathlib import Path
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parent.parent
BASE = {
    'fs/exec.c': ['ksu_handle_execveat', 'ksu_handle_post_execveat'],
    'fs/open.c': ['ksu_handle_faccessat'],
    'fs/stat.c': ['ksu_handle_stat', 'ksu_handle_newfstat_ret', 'ksu_handle_fstat64_ret'],
    'kernel/reboot.c': ['ksu_handle_sys_reboot'],
}
INCOMPATIBLE = {
    'fs/read_write.c': ['ksu_vfs_read_hook'],
    'security/selinux/hooks.c': ['is_ksu_transition'],
    'security/security.c': ['ksu_handle_rename'],
}

def auto_lsm(version):
    return 'y' if version < (6, 8) else 'n'


def code_only(source):
    return re.sub(r'/\*.*?\*/|//[^\n]*', '', source, flags=re.S)


def has_call(source, name):
    code = code_only(source)
    # Remove prototypes; finding an extern alone must never pass the gate.
    code = re.sub(r'\bextern\b[^;]*;', '', code, flags=re.S)
    return re.search(r'\b' + re.escape(name) + r'\s*\(', code) is not None


def kernel_root(source):
    for relative in ('', 'common', 'kernel_platform/common'):
        candidate = source / relative
        if (candidate / 'fs/exec.c').is_file() and (candidate / 'Makefile').is_file():
            return candidate
    raise ValueError(f'Cannot locate kernel source in {source}')


def version_of(source):
    make = (source / 'Makefile').read_text()
    return tuple(int(re.search(r'^' + field + r'\s*=\s*(\d+)', make, re.M)[1])
                 for field in ('VERSION', 'PATCHLEVEL'))


def missing_hooks(source, version):
    required = {path: list(names) for path, names in BASE.items()}
    if auto_lsm(version) == 'n':
        required['kernel/sys.c'] = ['ksu_handle_setresuid']
        required['fs/read_write.c'] = ['ksu_handle_sys_read']
    missing = []
    for path, names in required.items():
        content = (source / path).read_text()
        if re.search(r'\b(?:CONFIG_KSU_SUSFS\w*|susfs_\w+)\b', code_only(content)):
            raise ValueError(f'{path}: retired filesystem hooks remain; use a clean source checkout')
        for name in names:
            if not has_call(content, name):
                missing.append(f'{path}: {name}')
    for path, names in INCOMPATIBLE.items():
        content = code_only((source / path).read_text())
        for name in names:
            if re.search(r'\b' + name + r'\b', content):
                raise ValueError(f'Incompatible legacy hook in {path}: {name}')
    return missing


def required_config(version):
    return {
        'CONFIG_KSU': 'y',
        'CONFIG_KSU_MANUAL_HOOK': 'y',
        'CONFIG_KSU_TRACEPOINT_HOOK': 'n',
        'CONFIG_KSU_MANUAL_HOOK_AUTO_SETUID_HOOK': auto_lsm(version),
        'CONFIG_KSU_MANUAL_HOOK_AUTO_INITRC_HOOK': auto_lsm(version),
        'CONFIG_KSU_MANUAL_HOOK_AUTO_INPUT_HOOK': 'y',
    }


def config_values(path):
    values = {}
    for line in path.read_text().splitlines():
        if line.startswith('CONFIG_') and '=' in line:
            key, value = line.split('=', 1)
            values[key] = value
        elif line.startswith('# CONFIG_') and line.endswith(' is not set'):
            values[line[2:-11]] = 'n'
    return values


def check_config(path, version):
    values = config_values(path)
    for key, wanted in required_config(version).items():
        if values.get(key, 'n') != wanted:
            raise ValueError(f'{path}: {key} must be {wanted}, got {values.get(key, "n")}')
    for key, value in values.items():
        if 'SUSFS' in key and value not in ('n', '""'):
            raise ValueError(f'{path}: removed feature is still enabled: {key}')


def configure(path, version):
    entries = required_config(version)
    lines = []
    for line in path.read_text().splitlines():
        match = re.match(r'(?:# )?(CONFIG_\w+)(?:=| is not set)', line)
        if 'SUSFS' in line or (match and match[1] in entries):
            continue
        lines.append(line)
    for key, value in entries.items():
        lines.append(f'{key}={value}' if value != 'n' else f'# {key} is not set')
    path.write_text('\n'.join(lines) + '\n')
    check_config(path, version)


def integrate(source, project, defconfig):
    source = kernel_root(source)
    version = version_of(source)
    missing = missing_hooks(source, version)
    if missing:
        patch = ROOT / 'configs/bakasu' / f'{project}.patch'
        if not patch.is_file():
            raise ValueError('Missing BakaSU hooks: ' + ', '.join(missing))
        command = ['patch', '-p1', '--batch', '--forward', '--fuzz=0', '-i', str(patch)]
        # Verify the entire patch before writing; reject partially integrated trees.
        subprocess.run(command + ['--dry-run'], cwd=source, check=True)
        subprocess.run(command, cwd=source, check=True)
    missing = missing_hooks(source, version)
    if missing:
        raise ValueError('Missing BakaSU hooks after integration: ' + ', '.join(missing))
    config = source / 'arch/arm64/configs' / defconfig
    configure(config, version)
    print(f'{project}: BakaSU manual hooks verified ({version[0]}.{version[1]})')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source', type=Path, required=True)
    parser.add_argument('--project', required=True)
    parser.add_argument('--defconfig', required=True)
    parser.add_argument('--check-config', type=Path)
    args = parser.parse_args()
    if args.check_config:
        source = kernel_root(args.source)
        missing = missing_hooks(source, version_of(source))
        if missing:
            raise ValueError('Missing BakaSU hooks: ' + ', '.join(missing))
        check_config(args.check_config, version_of(source))
    else:
        integrate(args.source, args.project, args.defconfig)


if __name__ == '__main__':
    try:
        main()
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        sys.exit(str(error))
