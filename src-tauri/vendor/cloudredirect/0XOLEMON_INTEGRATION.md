# 0xoLemon integration

This directory contains the unmodified upstream CloudRedirect source snapshot supplied by the user.
0xoLemon builds only the native Windows targets `cloud_redirect`, `cloud_redirect_cli`, and `cloud760_tool`.
The WPF UI and CloudRedirect updater/news surfaces are not bundled or launched; 0xoLemon supplies the integrated React/Tauri UI and updater.

- Upstream-reported version: `2.6.3`
- Source commit recorded in the supplied archive: `9d0dbbf48f349a4172d2d47a936bb41c5f5ecff6`
- Supplied archive SHA-256: `3d92bf74d3166aec2457456452f381734de5406d8f42f1b8fe733dd68b55b2ae`
- Official v2.6.3 release tag commit: `ecc84a0`

The supplied archive is a post-release `master` snapshot that still reports version 2.6.3; it is not byte-identical to the official v2.6.3 tag.

## Distribution notice

No `LICENSE` file was present in the supplied upstream snapshot. 0xoLemon must obtain explicit redistribution permission from the CloudRedirect copyright holder before publicly bundling or distributing the upstream source or compiled binaries.
