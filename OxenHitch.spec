"""Package the Windows React and Python application with PyInstaller.

| Stage | Mechanism | Tag | Ref |
| --- | --- | --- | --- |
| ASSET | Collect the Vite frontend and tray icon | [mandatory] | R1 |
| PYTHONNET | Include the dynamically imported CLR bridge and runtime files | [mandatory] | R2 |
| ANALYZE | Resolve Python modules and native dependencies | [mandatory] | R3 |
| COLLECT | Assemble the Windows onedir application | [mandatory] | R4 |

EARS guards: R1 WHEN the Windows frontend is built, THEN relative URLs resolve from the packaged file page.
R2 WHEN pywebview initializes EdgeChromium, THEN ``clr`` and Python.Runtime are importable.
R3 IF analysis misses a required runtime import, THEN the build is incomplete.
CLOSED: no Rust/Tauri runtime or compiler is included.
Preconditions: run from the repository root after ``pnpm build:windows`` and install ``requirements.txt``.
Failure modes: absent frontend assets or pywebview runtime imports fail package verification.
"""

# -*- mode: python ; coding: utf-8 -*-


a = Analysis(
    ['hitch_main.py'],
    pathex=[],
    binaries=[],
    datas=[('dist', 'dist'), ('src-tauri/icons/tray.png', 'src-tauri/icons')],
    hiddenimports=['clr'],
    hookspath=[],
    hooksconfig={},
    runtime_hooks=[],
    excludes=[],
    noarchive=False,
    optimize=0,
)
pyz = PYZ(a.pure)

exe = EXE(
    pyz,
    a.scripts,
    [],
    exclude_binaries=True,
    name='OxenHitch',
    debug=False,
    bootloader_ignore_signals=False,
    strip=False,
    upx=True,
    console=False,
    disable_windowed_traceback=False,
    argv_emulation=False,
    target_arch=None,
    codesign_identity=None,
    entitlements_file=None,
    icon=['src-tauri/icons/icon.ico'],
)
coll = COLLECT(
    exe,
    a.binaries,
    a.datas,
    strip=False,
    upx=True,
    upx_exclude=[],
    name='OxenHitch',
)
