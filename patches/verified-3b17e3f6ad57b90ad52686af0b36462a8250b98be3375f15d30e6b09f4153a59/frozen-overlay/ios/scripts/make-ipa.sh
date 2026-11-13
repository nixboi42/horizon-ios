#!/bin/sh
# Package an unsigned, re-signable HorizonIOS.ipa and print its SHA-256.
#
# The IPA contains no Minecraft data. It becomes playable after the owner copies
# their own prepared HorizonResources folder into the app's Documents directory.
# Use `--signed` on build-app.sh (or Xcode) when installing with a development team.
#
#   ./ios/scripts/make-ipa.sh
#   ./ios/scripts/make-ipa.sh --app PATH/TO/HorizonIOS.app --output dist/HorizonIOS.ipa
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
APP=
OUTPUT=
ALLOW_RESOURCES=0

while [ $# -gt 0 ]; do
  case "$1" in
    -a|--app) APP=$2; shift 2 ;;
    -o|--output) OUTPUT=$2; shift 2 ;;
    --allow-resources) ALLOW_RESOURCES=1; shift ;;
    -h|--help) sed -n '2,10p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "Unknown argument: $1" >&2; exit 2 ;;
  esac
done

if [ -z "$APP" ]; then
  DERIVED=${HORIZON_DERIVED_DATA:-"$ROOT/.local/xcode-device"}
  "$ROOT/ios/scripts/build-app.sh" --platform device --derived-data "$DERIVED" >&2
  APP="$DERIVED/Build/Products/Release-iphoneos/HorizonIOS.app"
fi
APP=$(CDPATH= cd -- "$APP" && pwd)
[ -d "$APP" ] || { echo "App bundle not found: $APP" >&2; exit 1; }

PLIST="$APP/Info.plist"
# The Xcode project copies this folder to the app root. A clean checkout has
# only `.gitkeep`; a developer's build machine may have prepared data here.
RESOURCES="$APP/PreparedResources"
if [ -e "$RESOURCES/mobile-import.json" ] && [ "$ALLOW_RESOURCES" != 1 ]; then
  echo "Stripping private prepared resources from the public archive."
fi

VERSION=$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$PLIST")
BUILD=$(/usr/libexec/PlistBuddy -c 'Print :CFBundleVersion' "$PLIST")
MINIMUM=$(/usr/libexec/PlistBuddy -c 'Print :MinimumOSVersion' "$PLIST" 2>/dev/null || echo unknown)
EXECUTABLE=$(/usr/libexec/PlistBuddy -c 'Print :CFBundleExecutable' "$PLIST")
[ -f "$APP/$EXECUTABLE" ] || { echo "Executable missing: $APP/$EXECUTABLE" >&2; exit 1; }

OUTPUT=${OUTPUT:-"$ROOT/dist/HorizonIOS-$VERSION-unsigned.ipa"}
mkdir -p "$(dirname "$OUTPUT")"
OUTPUT=$(CDPATH= cd -- "$(dirname "$OUTPUT")" && pwd)/$(basename "$OUTPUT")
WORK=$(mktemp -d "${TMPDIR:-/tmp}/horizon-ipa.XXXXXX")
trap 'rm -rf "$WORK"' EXIT INT TERM

mkdir -p "$WORK/Payload"
# ditto preserves bundle structure and resource forks better than cp -R.
ditto "$APP" "$WORK/Payload/HorizonIOS.app"
# A signature and provisioning profile are per-machine credentials; never ship them.
rm -rf "$WORK/Payload/HorizonIOS.app/_CodeSignature" \
       "$WORK/Payload/HorizonIOS.app/embedded.mobileprovision" \
       "$WORK/Payload/HorizonIOS.app/SC_Info"
if codesign -dv "$WORK/Payload/HorizonIOS.app/$EXECUTABLE" >/dev/null 2>&1; then
  codesign --remove-signature "$WORK/Payload/HorizonIOS.app/$EXECUTABLE"
fi
# Remove the linker debug map, which records paths to native object files.
xcrun strip -S "$WORK/Payload/HorizonIOS.app/$EXECUTABLE"
if [ "$ALLOW_RESOURCES" != 1 ]; then
  rm -rf "$WORK/Payload/HorizonIOS.app/PreparedResources"
fi
if [ "$ALLOW_RESOURCES" = 1 ]; then
  echo "WARNING: private prepared resources are included; never distribute this archive." >&2
  "$ROOT/ios/scripts/verify-app.sh" "$WORK/Payload/HorizonIOS.app" --private
else
  "$ROOT/ios/scripts/verify-app.sh" "$WORK/Payload/HorizonIOS.app"
  "$ROOT/ios/scripts/check_release.py" --tree "$WORK/Payload/HorizonIOS.app"
fi
rm -f "$OUTPUT"
(cd "$WORK" && zip -qry "$OUTPUT" Payload)

echo "IPA:      $OUTPUT"
echo "Version:  $VERSION ($BUILD)"
echo "Minimum:  iOS $MINIMUM"
echo "Arch:     $(lipo -archs "$APP/$EXECUTABLE")"
echo "Devices:  $(/usr/libexec/PlistBuddy -c 'Print :UIDeviceFamily' "$PLIST" | tr -d ' \n' | sed 's/1/iPhone/;s/2/, iPad/')"
echo "SHA-256:  $(shasum -a 256 "$OUTPUT" | awk '{print $1}')"
echo
echo "This IPA is unsigned. Install it with Sideloadly, AltStore, TrollStore,"
echo "or re-sign it with your own Apple ID/development team."
if [ "$ALLOW_RESOURCES" != 1 ]; then
  echo "Copy your HorizonResources folder into the app’s Documents directory before playing."
fi
