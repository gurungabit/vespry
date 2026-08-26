#!/usr/bin/env bash
# Local release build signed with the stable "Vespry Dev Signing" identity so
# macOS TCC grants (Accessibility/Microphone) survive rebuilds. CI stays
# ad-hoc signed. Create the identity once with:
#   openssl req -x509 -newkey rsa:2048 -keyout key.pem -out cert.pem -days 3650 \
#     -nodes -subj "/CN=Vespry Dev Signing" \
#     -addext "keyUsage=digitalSignature" -addext "extendedKeyUsage=codeSigning"
#   openssl pkcs12 -export -out cert.p12 -inkey key.pem -in cert.pem -password pass:vespry
#   security import cert.p12 -k ~/Library/Keychains/login.keychain-db -P vespry -T /usr/bin/codesign
set -euo pipefail
cd "$(dirname "$0")/.."

pnpm tauri build
APP=src-tauri/target/release/bundle/macos/Vespry.app
codesign --force --deep --options runtime \
  --entitlements src-tauri/Entitlements.plist \
  -s "Vespry Dev Signing" "$APP"
echo "Built and signed: $APP"
