"""Refuse a Windows binary that needs the Visual C++ runtime installed.

A clean Windows has no VCRUNTIME140.dll. A build that imports it exits with
STATUS_DLL_NOT_FOUND before drawing anything, which the winget validator
caught and which any user on a fresh machine would have hit the same way.
The runtime is linked statically to avoid that, and this is the check that
proves the finished binary actually came out that way, since the flag lives
in two places and an environment variable can silently drop it.

    python packaging/check-imports.py path/to/snag.exe
"""

import sys

try:
    import pefile
except ImportError:
    sys.exit("pefile is not installed: python -m pip install pefile")

if len(sys.argv) != 2:
    sys.exit("usage: check-imports.py <snag.exe>")

pe = pefile.PE(sys.argv[1], fast_load=True)
pe.parse_data_directories(
    directories=[pefile.DIRECTORY_ENTRY["IMAGE_DIRECTORY_ENTRY_IMPORT"]]
)
imports = [entry.dll.decode().lower() for entry in pe.DIRECTORY_ENTRY_IMPORT]

# vcruntime and msvcp are the redistributable. The api-ms-win-crt-* set is the
# universal CRT, which Windows 10 and later ship, but its presence means the
# runtime was linked dynamically after all, so it is refused for the same reason.
needs_redist = [
    dll
    for dll in imports
    if dll.startswith(("vcruntime", "msvcp", "api-ms-win-crt"))
]

if needs_redist:
    print("the binary depends on the visual c++ runtime, which a clean windows lacks:")
    for dll in needs_redist:
        print(f"  {dll}")
    print("the static crt flag was lost: see snag/.cargo/config.toml")
    sys.exit(1)

print(f"ok: {len(imports)} imports, none from the visual c++ runtime")
