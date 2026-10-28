# Releasing

Push a tag and the release workflow builds every bundle into a draft release:

```sh
git tag v0.1.0 && git push origin v0.1.0
```

Artifacts: macOS `.dmg` (Apple Silicon and Intel), Windows `.msi`/`.exe`,
Linux `.AppImage`/`.deb`, plus a standalone `journal` CLI per platform.
Review the draft on GitHub, then publish.

## macOS signing and notarization

Unsigned builds open only after right-click → Open. To sign and notarize in
CI, add these repository secrets (Settings → Secrets → Actions, or `gh secret set`):

| secret | value |
|---|---|
| `APPLE_SIGNING_IDENTITY` | `Developer ID Application: NAME (TEAMID)` as shown by `security find-identity -v -p codesigning` |
| `APPLE_CERTIFICATE` | base64 of a `.p12` export of that certificate and its private key: Keychain Access → right-click the certificate → Export → `.p12`, then `base64 -i cert.p12 | pbcopy` |
| `APPLE_CERTIFICATE_PASSWORD` | the password you set on the `.p12` |
| `APPLE_ID` | the Apple ID email of the developer account |
| `APPLE_PASSWORD` | an app-specific password from appleid.apple.com (not the account password) |
| `APPLE_TEAM_ID` | the ten-character team id |

Local signed build (no notarization):

```sh
APPLE_SIGNING_IDENTITY="Developer ID Application: NAME (TEAMID)" npm run tauri build
```

Add `APPLE_ID`, `APPLE_PASSWORD` and `APPLE_TEAM_ID` to that command to notarize locally too.
