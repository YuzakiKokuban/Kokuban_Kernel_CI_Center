#!/usr/bin/env python3
"""Integrate a pinned SUSFS patchset as a separate BakaSU hook mode."""
import argparse
import hashlib
import fcntl
import importlib.util
import json
from pathlib import Path, PurePosixPath
import re
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location('manual_hooks', ROOT / 'scripts/bakasu-hooks.py')
hooks = importlib.util.module_from_spec(spec)
spec.loader.exec_module(hooks)

REQUIRED_HOOKS = {
    'kernel/sys.c': ['ksu_handle_setresuid'],
    'fs/exec.c': ['ksu_handle_execveat'],
    'fs/open.c': ['ksu_handle_faccessat'],
    'fs/read_write.c': ['ksu_handle_sys_read'],
    'fs/stat.c': ['ksu_handle_stat'],
    'kernel/reboot.c': ['ksu_handle_sys_reboot'],
    'drivers/input/input.c': ['ksu_handle_input_handle_event'],
}
REQUIRED_CONFIG = {
    'CONFIG_KSU': 'y',
    'CONFIG_KSU_SUSFS': 'y',
    'CONFIG_KSU_MANUAL_HOOK': 'n',
    'CONFIG_KSU_TRACEPOINT_HOOK': 'n',
    'CONFIG_KSU_SUSFS_SUS_PATH': 'y',
    'CONFIG_KSU_SUSFS_SUS_MOUNT': 'y',
    'CONFIG_KSU_SUSFS_SUS_KSTAT': 'y',
    'CONFIG_KSU_SUSFS_SPOOF_UNAME': 'y',
    'CONFIG_KSU_SUSFS_ENABLE_LOG': 'y',
    'CONFIG_KSU_SUSFS_HIDE_KSU_SUSFS_SYMBOLS': 'y',
    'CONFIG_KSU_SUSFS_SPOOF_CMDLINE_OR_BOOTCONFIG': 'y',
    'CONFIG_KSU_SUSFS_OPEN_REDIRECT': 'y',
    'CONFIG_KSU_SUSFS_SUS_MAP': 'y',
}


def check_config(path):
    values = hooks.config_values(path)
    for key, wanted in REQUIRED_CONFIG.items():
        if values.get(key, 'n') != wanted:
            raise ValueError(f'{path}: {key} must be {wanted}, got {values.get(key, "n")}')


def configure(path):
    lines = []
    for line in path.read_text().splitlines():
        match = re.match(r'(?:# )?(CONFIG_\w+)(?:=| is not set)', line)
        if match and (match[1] in REQUIRED_CONFIG or match[1].startswith('CONFIG_KSU_MANUAL_HOOK_AUTO_')):
            continue
        lines.append(line)
    lines.extend(f'{key}={value}' if value != 'n' else f'# {key} is not set'
                 for key, value in REQUIRED_CONFIG.items())
    path.write_text('\n'.join(lines) + '\n')
    check_config(path)


def patch_paths(content):
    paths = set()
    for line in content.splitlines():
        if not line.startswith(('--- ', '+++ ')):
            continue
        value = line[4:].split('\t', 1)[0]
        if value == '/dev/null':
            continue
        if not value.startswith(('a/', 'b/')):
            raise ValueError(f'Unsupported patch path: {value}')
        value = value[2:]
        path = PurePosixPath(value)
        if path.is_absolute() or '..' in path.parts or not path.parts:
            raise ValueError(f'Unsafe patch path: {value}')
        paths.add(value)
    if not paths:
        raise ValueError('Patch contains no file changes')
    return paths


def apply_checked_patch(source, patch, reverse=False):
    command = ['patch', '-p1', '--batch', '--fuzz=0', '--no-backup-if-mismatch',
               '--reverse' if reverse else '--forward', '-i', str(patch.resolve())]
    result = subprocess.run(command + ['--dry-run'], cwd=source, capture_output=True, text=True)
    if result.returncode:
        raise ValueError(f'Patch does not match {source}:\n{result.stdout}{result.stderr}')
    subprocess.run(command, cwd=source, check=True)


def check_hooks(source):
    for relative, names in REQUIRED_HOOKS.items():
        content = (source / relative).read_text()
        for name in names:
            if not hooks.has_call(content, name):
                raise ValueError(f'{relative}: missing SUSFS call {name}')
        if re.search(r'\b(?:ksu_vfs_read_hook|ksu_input_hook|ksu_execveat_hook|ksu_init_rc_hook)\b',
                     hooks.code_only(content)):
            raise ValueError(f'{relative}: incompatible legacy SUSFS hook')


def checkout_patchset(config, destination, offline):
    revision = config['revision']
    if not re.fullmatch(r'[0-9a-f]{40}', revision):
        raise ValueError('SUSFS revision must be a full immutable Git commit')
    if not destination.exists():
        if offline:
            raise ValueError('SUSFS patchset is not cached; retry without --offline')
        subprocess.run(['git', 'init', str(destination)], check=True, capture_output=True)
        subprocess.run(['git', '-C', str(destination), 'remote', 'add', 'origin', config['repo']], check=True)
    present = subprocess.run(['git', '-C', str(destination), 'cat-file', '-e', revision + '^{commit}'],
                             capture_output=True).returncode == 0
    if not present:
        if offline:
            raise ValueError(f'SUSFS revision {revision} is not cached')
        subprocess.run(['git', '-C', str(destination), 'fetch', '--depth=1', 'origin', revision], check=True)
    subprocess.run(['git', '-C', str(destination), 'checkout', '--detach', '--force', revision], check=True)
    return destination


def integrate(source, project, defconfig, offline=False, patchset=None):
    config = json.loads((ROOT / 'configs/projects.json').read_text())[project].get('susfs')
    if not config:
        raise ValueError(f'{project}: no SUSFS patchset configured')
    source = hooks.kernel_root(source.resolve())
    expected = re.search(r'[-_](\d+\.\d+)\.patch$', config['patch'])
    version = '.'.join(map(str, hooks.version_of(source)))
    if not expected or expected[1] != version:
        raise ValueError(f'{project}: SUSFS patchset does not target Linux {version}')
    marker = source / '.kokuban-susfs.json'
    identity = {key: config[key] for key in ('repo', 'revision', 'patch')}
    if config.get('local_patch'):
        identity['local_patch_sha256'] = hashlib.sha256((ROOT / config['local_patch']).read_bytes()).hexdigest()
    if (marker.exists() and (source / 'fs/susfs.c').exists()
            and 'CONFIG_KSU_SUSFS' in (source / 'fs/exec.c').read_text()):
        if json.loads(marker.read_text()) != identity:
            raise ValueError('SUSFS source revision changed; use a clean source checkout')
        check_hooks(source)
        configure(source / 'arch/arm64/configs' / defconfig)
        return
    cache = ROOT / '.superpowers/susfs-cache' / config['revision']
    if patchset is None:
        cache.parent.mkdir(parents=True, exist_ok=True)
        with (cache.parent / f'{config["revision"]}.lock').open('a') as lock:
            fcntl.flock(lock, fcntl.LOCK_EX)
            return integrate(source, project, defconfig, offline,
                             checkout_patchset(config, cache, offline))
    patch = (ROOT / config['local_patch']) if config.get('local_patch') else patchset / config['patch']
    paths = patch_paths(patch.read_text())
    manual_patch = ROOT / 'configs/bakasu' / f'{project}.patch'
    manual_present = any('CONFIG_KSU_MANUAL_HOOK' in (source / name).read_text()
                         for name in REQUIRED_HOOKS)
    if manual_present and not manual_patch.exists():
        raise ValueError(f'{project}: existing manual hooks need a reviewed reverse patch')
    if manual_present:
        paths |= patch_paths(manual_patch.read_text())
    payloads = {}
    for directory in ('fs', 'include/linux'):
        for file in (patchset / 'kernel_patches' / directory).glob('*'):
            if file.is_file():
                payloads[f'{directory}/{file.name}'] = file
    paths |= payloads.keys()
    paths.add(f'arch/arm64/configs/{defconfig}')
    # Stage every affected file. No failed hunk or hook check may change source.
    with tempfile.TemporaryDirectory(prefix='kokuban-susfs-') as tmp:
        stage = Path(tmp)
        for relative in paths:
            original = source / relative
            target = stage / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            if original.is_file():
                shutil.copy2(original, target)
        if manual_present:
            apply_checked_patch(stage, manual_patch, reverse=True)
        apply_checked_patch(stage, patch)
        for relative, file in payloads.items():
            shutil.copy2(file, stage / relative)
        check_hooks(stage)
        configure(stage / 'arch/arm64/configs' / defconfig)
        for relative in paths:
            target = source / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(stage / relative, target)
    marker.write_text(json.dumps(identity, indent=2) + '\n')
    print(f'{project}: SUSFS integrated at {config["revision"]}')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source', type=Path, required=True)
    parser.add_argument('--project', required=True)
    parser.add_argument('--defconfig', required=True)
    parser.add_argument('--check-config', type=Path)
    parser.add_argument('--offline', action='store_true')
    parser.add_argument('--patchset', type=Path)
    args = parser.parse_args()
    if args.check_config:
        check_hooks(hooks.kernel_root(args.source.resolve()))
        check_config(args.check_config)
    else:
        integrate(args.source, args.project, args.defconfig, args.offline, args.patchset)


if __name__ == '__main__':
    try:
        main()
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        raise SystemExit(str(error))
