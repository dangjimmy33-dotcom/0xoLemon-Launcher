0xoLemon offline prerequisite bundle

The release build generates this directory from dependency-bundle.json.
Installer binaries are downloaded only from pinned vendor endpoints, verified
with SHA-256 and Authenticode publisher checks, then packaged as Tauri resources.
The catalog covers Visual C++ 2008/2010/2012/2013/2015-2022, DirectX June 2010,
.NET Framework 4.8.1, XNA 4.0 Refresh, NVIDIA PhysX and OpenAL 1.1.

Generated executable files are intentionally ignored by Git. Run:

  powershell -NoProfile -ExecutionPolicy Bypass -File src-tauri/prepare-dependencies.ps1

The launcher installs only packages declared by a game's installed manifest or
its production game profile. It does not install every package unconditionally.
