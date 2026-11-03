#!/usr/bin/env python3
"""Execute Pixel 4 SUSFS kernel C regressions with small host-side stand-ins.

Default input is the reviewed local patch (no checkout/network required).
KOKUBAN_TEST_KERNEL_SOURCE selects an already patched kernel tree instead.
"""
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]


def patched_files():
    source = os.environ.get('KOKUBAN_TEST_KERNEL_SOURCE')
    if source:
        return {name: (Path(source) / name).read_text()
                for name in ('fs/stat.c', 'fs/namespace.c')}
    # Preserve hunk boundaries: never manufacture a function across omitted code.
    files, chunks, name = {}, [], None
    for line in (ROOT / 'configs/susfs/floral_sm8150.patch').read_text().splitlines():
        if line.startswith('+++ b/'):
            name = line[6:]
            chunks = files.setdefault(name, [])
        elif line.startswith('@@ '):
            chunks.append([])
        elif chunks and line.startswith((' ', '+')):
            chunks[-1].append(line[1:])
    return {name: '\n/* OMITTED PATCH CONTEXT */\n'.join('\n'.join(c) for c in chunks)
            for name, chunks in files.items()}


def function(text, name):
    match = re.search(r'^[\w *]+\b' + re.escape(name) + r'\s*\([^;]*?\)\s*\{', text, re.M)
    if not match:
        raise AssertionError(f'patch needs full function context for {name}')
    start = text.index('{', match.start())
    # Ignore braces in comments and literals while finding the function end.
    tokens = re.compile(r'/\*.*?\*/|//[^\n]*|"(?:\\.|[^"\\])*"|\'(?:\\.|[^\'\\])*\'|[{}]', re.S)
    depth = 0
    for token in tokens.finditer(text, start):
        if token.group() == '{':
            depth += 1
        elif token.group() == '}':
            depth -= 1
            if not depth:
                result = text[match.start():token.end()]
                if 'OMITTED PATCH CONTEXT' in result:
                    raise AssertionError(f'patch needs contiguous function context for {name}')
                return result
    raise AssertionError(f'incomplete function {name}')


STAT_STUBS = r'''
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#define CONFIG_KSU_SUSFS_SUS_KSTAT 1
typedef uint32_t u32;
#define STATX_BASIC_STATS 0x7ffU
#define STATX_ALL 0xfffU
#define STATX_SUS_KSTAT 0x10000000U
#define STATX_SUS_KSTAT_FUSE 0x20000000U
#define KSTAT_QUERY_FLAGS 0x6000U
struct path; struct kstat { u32 result_mask; int size; };
struct inode_operations { int (*getattr)(const struct path *,struct kstat *,u32,unsigned); };
struct inode { struct inode_operations *i_op; };
struct dentry { struct inode *inode; }; struct path { struct dentry *dentry; };
#define d_backing_inode(d) ((d)->inode)
static bool app=true, marked=true, fuse=false;
static int getter_error, overwrite_mask, spoof_calls, wrong_spoof;
static bool susfs_is_current_app_uid(void) { return app; }
static bool susfs_is_inode_sus_kstat(struct inode *i,bool *f) { (void)i; *f=fuse; return marked; }
static void generic_fillattr(struct inode *i,struct kstat *s) { (void)i; s->size=5; }
static void susfs_sus_kstat_spoof_generic_fillattr(struct inode *i,struct kstat *s,u32 m) {
 (void)i; spoof_calls++; wrong_spoof += m != (fuse?STATX_SUS_KSTAT_FUSE:STATX_SUS_KSTAT); s->size=99;
}
static int getter(const struct path *p,struct kstat *s,u32 m,unsigned q) {
 (void)p; (void)m; (void)q; s->size=5; if(overwrite_mask) s->result_mask=0x123U; return getter_error;
}
'''

MOUNT_STUBS = r'''
#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#define CONFIG_KSU_SUSFS_SUS_MOUNT 1
#define VFSMOUNT_MNT_FLAGS_KSU_UNSHARED_MNT 0x80000000U
#define MNT_WRITE_HOLD 1U
#define MNT_MARKED 2U
#define MNT_INTERNAL 4U
#define GFP_KERNEL 0
#define GFP_KERNEL_ACCOUNT 0
#define unlikely(x) (x)
struct mount {
 struct { unsigned mnt_flags; } mnt;
 int mnt_id, mnt_count, mnt_writers;
 char *mnt_devname;
 int mnt_hash,mnt_child,mnt_mounts,mnt_list,mnt_expire,mnt_share;
 int mnt_slave_list,mnt_slave,mnt_mp_list,mnt_umounting,mnt_umount;
};
static int mnt_cache, mnt_id_lock, mnt_id_ida, mnt_id_start=1000, freed_id=-1;
#define kmem_cache_zalloc(cache,flags) calloc(1,sizeof(struct mount))
#define kmem_cache_free(cache,p) free(p)
#define kstrdup_const(s,flags) strdup(s)
#define kfree_const(p) free((void *)(p))
#define INIT_HLIST_NODE(p) ((void)(p))
#define INIT_LIST_HEAD(p) ((void)(p))
#define init_fs_pin(p,fn) ((void)(p))
#define spin_lock(p) ((void)(p))
#define spin_unlock(p) ((void)(p))
static void ida_remove(int *ida,int id) { (void)ida; freed_id=id; }
'''


class Pixel4SusfsC(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.files = patched_files()
        cls.cc = shutil.which(os.environ.get('CC', 'cc'))
        if not cls.cc:
            raise RuntimeError('host C compiler required (set CC)')

    def run_c(self, source):
        with tempfile.TemporaryDirectory(prefix='pixel4-susfs-c-') as directory:
            src, exe = Path(directory) / 'test.c', Path(directory) / 'test'
            src.write_text(source)
            result = subprocess.run([self.cc, '-std=gnu11', '-O0', str(src), '-o', str(exe)],
                                    capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            result = subprocess.run([str(exe)], capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_kstat_preserves_public_mask_and_spoofs_after_getattr(self):
        body = function(self.files['fs/stat.c'], 'vfs_getattr_nosec')
        self.run_c(STAT_STUBS + body + r'''
int main(void) {
 struct inode_operations ops={getter}; struct inode inode={&ops};
 struct dentry d={&inode}; struct path p={&d}; struct kstat s;
 int failures=0;
 for(int has_getter=0;has_getter<2;has_getter++)
 for(int is_fuse=0;is_fuse<2;is_fuse++)
 for(int overwrite=0;overwrite<2;overwrite++)
 for(int error=0;error<2;error++)
 for(int selected=0;selected<2;selected++) {
  ops.getattr=has_getter?getter:NULL; fuse=is_fuse; overwrite_mask=overwrite;
  getter_error=error?-5:0; marked=selected; spoof_calls=wrong_spoof=0;
  int ret=vfs_getattr_nosec(&p,&s,STATX_ALL,0);
  int expected_ret=has_getter?getter_error:0;
  u32 expected_mask=has_getter&&overwrite?0x123U:STATX_BASIC_STATS;
  int expected_spoof=selected&&!expected_ret;
  if(ret!=expected_ret || s.result_mask!=expected_mask || spoof_calls!=expected_spoof || wrong_spoof ||
     (!expected_ret && s.size!=(expected_spoof?99:5))) {
   fprintf(stderr,"getattr=%d fuse=%d overwrite=%d error=%d selected=%d: ret=%d mask=%x spoof=%d wrong=%d size=%d\n",
    has_getter,is_fuse,overwrite,error,selected,ret,s.result_mask,spoof_calls,wrong_spoof,s.size); failures++;
  }
 }
 return failures?1:0;
}
''')

    def test_borrowed_mount_id_survives_early_clone_cleanup(self):
        text = self.files['fs/namespace.c']
        self.run_c(MOUNT_STUBS + function(text, 'mnt_free_id') + '\n' +
                   function(text, 'susfs_alloc_unshare_ksu_vfsmnt') + r'''
int main(void) {
 struct mount *mnt=susfs_alloc_unshare_ksu_vfsmnt(NULL,42);
 if(!mnt) return 2;
 /* Same release used if clone_mnt_data fails before flags are copied. */
 mnt_free_id(mnt);
 int bad=freed_id!=-1 || !(mnt->mnt.mnt_flags&VFSMOUNT_MNT_FLAGS_KSU_UNSHARED_MNT);
 if(bad) fprintf(stderr,"borrowed ID freed=%d flags=%x\n",freed_id,mnt->mnt.mnt_flags);
 free(mnt); return bad;
}
''')

    def test_fresh_clone_releases_its_own_id(self):
        text = self.files['fs/namespace.c']
        begin = text.index('\tmnt->mnt.mnt_flags = old->mnt.mnt_flags;')
        end = text.index("\t/* Don't allow unprivileged", begin)
        flags = text[begin:end]
        self.assertNotIn('OMITTED PATCH CONTEXT', flags)
        self.run_c(MOUNT_STUBS + function(text, 'mnt_free_id') + r'''
static void copy_flags(struct mount *mnt,struct mount *old,bool is_mnt_ksu_unshared) {
''' + flags + r'''
}
int main(void) {
 struct mount old={0}, fresh={0}, borrowed={0};
 old.mnt.mnt_flags=VFSMOUNT_MNT_FLAGS_KSU_UNSHARED_MNT|0x100U|MNT_MARKED;
 fresh.mnt_id=43; borrowed.mnt_id=42;
 copy_flags(&fresh,&old,false); mnt_free_id(&fresh);
 if(freed_id!=43 || fresh.mnt.mnt_flags!=0x100U) {
  fprintf(stderr,"fresh ID freed=%d flags=%x\n",freed_id,fresh.mnt.mnt_flags); return 1;
 }
 freed_id=-1; copy_flags(&borrowed,&old,true); mnt_free_id(&borrowed);
 if(freed_id!=-1 || borrowed.mnt.mnt_flags!=(VFSMOUNT_MNT_FLAGS_KSU_UNSHARED_MNT|0x100U)) return 2;
 return 0;
}
''')


if __name__ == '__main__':
    unittest.main()
