# Local font assets

These variable WOFF2 files are pinned to the official Google Fonts-hosted assets
and served by the application at `/static/fonts/`. The app has no runtime font
CDN dependency.

| File | Family/version | Source | SHA-256 |
| --- | --- | --- | --- |
| `inter-var-latin.woff2` | Inter v20, Latin subset | https://fonts.gstatic.com/s/inter/v20/UcC73FwrK3iLTeHuS_nVMrMxCp50SjIa1ZL7.woff2 | `3100e775e8616cd2611beecfa23a4263d7037586789b43f035236a2e6fbd4c62` |
| `inter-var-latin-ext.woff2` | Inter v20, Latin-ext subset | https://fonts.gstatic.com/s/inter/v20/UcC73FwrK3iLTeHuS_nVMrMxCp50SjIa25L7SUc.woff2 | `34b9c504cab7a73e37b746343a449132e56cf7b5481af2cb81dc74dcff25c956` |
| `space-grotesk-var-latin.woff2` | Space Grotesk v22, Latin subset | https://fonts.gstatic.com/s/spacegrotesk/v22/V8mDoQDjQSkFtoMM3T6r8E7mPbF4Cw.woff2 | `0640890476fc1198ab4de571fb658de443c4d85b66466ec09534a8737ab1ce9d` |
| `space-grotesk-var-latin-ext.woff2` | Space Grotesk v22, Latin-ext subset | https://fonts.gstatic.com/s/spacegrotesk/v22/V8mDoQDjQSkFtoMM3T6r8E7mPb94C-s0.woff2 | `952dddb45d2f96f71cbf3b7f510b24379afc3c89ea02fcf89d377b45d62c0166` |

The family versions and subset URLs are pinned as observed when these files
were obtained. The files were downloaded with `curl -fL` and their SHA-256
checksums were verified locally.

## License

Both families are distributed under the SIL Open Font License, Version 1.1:

- [Inter — OFL.txt](https://github.com/rsms/inter/blob/master/LICENSE.txt)
- [Space Grotesk — OFL.txt](https://github.com/floriankarsten/space-grotesk/blob/master/OFL.txt)
- [SIL Open Font License text](https://openfontlicense.org/open-font-license-official-text/)

The binary files remain subject to their respective OFL notices.
