"""tools/cargo_run.py creates temp directories that inherit their parent DACL.

On Windows, CPython maps a directory's POSIX mode onto its security
descriptor: ``tempfile.mkdtemp`` asks for 0o700, so the directory gets a
protected, owner-only DACL. That drops the write grant inherited from the
enclosing workspace or the harness's private temp root, and the confined child
then cannot write the directory it just created. Under the harness sandbox the
cargo test recorder fails with PermissionError and cleanup fails with
WinError 5.
"""
from __future__ import annotations

import ctypes
from ctypes import wintypes
import os
from pathlib import Path
import secrets
import shutil
import tempfile
import unittest

from tools import cargo_run

SE_FILE_OBJECT = 1
DACL_SECURITY_INFORMATION = 4
SE_DACL_PROTECTED = 0x1000


def dacl_protected(path: Path) -> bool:
    """Whether `path` carries a DACL protected from inheritance (Windows)."""
    advapi32 = ctypes.WinDLL('advapi32', use_last_error=True)
    kernel32 = ctypes.WinDLL('kernel32', use_last_error=True)
    get_named_security_info = advapi32.GetNamedSecurityInfoW
    get_named_security_info.argtypes = [
        wintypes.LPWSTR, ctypes.c_int, wintypes.DWORD, ctypes.c_void_p, ctypes.c_void_p,
        ctypes.POINTER(ctypes.c_void_p), ctypes.c_void_p, ctypes.POINTER(ctypes.c_void_p)]
    get_named_security_info.restype = wintypes.DWORD
    kernel32.LocalFree.argtypes = [ctypes.c_void_p]
    kernel32.LocalFree.restype = ctypes.c_void_p
    descriptor = ctypes.c_void_p()
    result = get_named_security_info(
        str(path), SE_FILE_OBJECT, DACL_SECURITY_INFORMATION,
        None, None, None, None, ctypes.byref(descriptor))
    if result:
        raise OSError(result, f'GetNamedSecurityInfoW({path})')
    try:
        control = ctypes.cast(descriptor, ctypes.POINTER(ctypes.c_ushort))[1]
        return bool(control & SE_DACL_PROTECTED)
    finally:
        kernel32.LocalFree(descriptor)


@unittest.skipUnless(os.name == 'nt', 'Windows DACL inheritance')
class TemporaryDirectoryTests(unittest.TestCase):
    def parent_directory(self) -> Path:
        parent = Path(tempfile.gettempdir()) / f'cargo-run-{os.getpid()}-{secrets.token_hex(3)}'
        parent.mkdir()
        self.addCleanup(shutil.rmtree, parent, ignore_errors=True)
        return parent

    def test_the_created_directory_inherits_the_parent_dacl(self):
        parent = self.parent_directory()
        with cargo_run.temporary_directory('cargo-tests-', dir=parent) as created:
            self.assertTrue(created.is_dir(), created)
            self.assertFalse(
                dacl_protected(created),
                'a created temp directory must inherit the parent DACL, not a protected owner-only one')
            (created / 'tests.jsonl').write_text('record\n', encoding='utf-8')
            self.assertEqual((created / 'tests.jsonl').read_text(encoding='utf-8'), 'record\n')

    def test_it_does_not_take_tempfiles_mode_0700_directory(self):
        # The 0o700 mode is exactly what turns into the protected owner-only DACL.
        parent = self.parent_directory()
        original = tempfile.mkdtemp

        def refuse(*args, **kwargs):
            raise AssertionError('tempfile.mkdtemp asks for a protected 0o700 DACL')

        tempfile.mkdtemp = refuse
        try:
            with cargo_run.temporary_directory('cargo-tests-', dir=parent) as created:
                self.assertTrue(created.is_dir(), created)
        finally:
            tempfile.mkdtemp = original


if __name__ == '__main__':
    unittest.main()
