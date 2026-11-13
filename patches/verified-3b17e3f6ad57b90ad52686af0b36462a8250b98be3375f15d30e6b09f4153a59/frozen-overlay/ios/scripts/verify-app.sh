#!/bin/sh
# Check a built HorizonIOS.app before it is published or installed.
#
#   ./ios/scripts/verify-app.sh PATH/TO/HorizonIOS.app [--private]
#
# --private permits embedded prepared resources (local builds only).
set -eu

APP=${1:-}
[ -n "$APP" ] && [ -d "$APP" ] || { echo "usage: verify-app.sh PATH/TO/HorizonIOS.app [--private]" >&2; exit 2; }
ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
EXPECTED=${IPHONEOS_DEPLOYMENT_TARGET:-$(awk '$1 == "IPHONEOS_DEPLOYMENT_TARGET" {print $3}' "$ROOT/ios/Deployment.xcconfig")}
PRIVATE=0
[ "${2:-}" = "--private" ] && PRIVATE=1

PLIST="$APP/Info.plist"
EXECUTABLE=$(/usr/libexec/PlistBuddy -c 'Print :CFBundleExecutable' "$PLIST")
BUNDLE=$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$PLIST")
VERSION=$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$PLIST")
BUILD=$(/usr/libexec/PlistBuddy -c 'Print :CFBundleVersion' "$PLIST")
MINIMUM=$(/usr/libexec/PlistBuddy -c 'Print :MinimumOSVersion' "$PLIST" 2>/dev/null || true)
FAMILY=$(/usr/libexec/PlistBuddy -c 'Print :UIDeviceFamily' "$PLIST" 2>/dev/null || true)

fail() { echo "verification failed: $*" >&2; exit 1; }
[ -f "$APP/$EXECUTABLE" ] || fail "missing executable $EXECUTABLE"
[ -n "$BUNDLE" ] || fail "missing CFBundleIdentifier"
[ -n "$VERSION" ] || fail "missing CFBundleShortVersionString"
[ -n "$BUILD" ] || fail "missing CFBundleVersion"
[ "$MINIMUM" = "$EXPECTED" ] || fail "MinimumOSVersion is '$MINIMUM', expected $EXPECTED"

ARCHS=$(xcrun lipo -archs "$APP/$EXECUTABLE")
[ "$ARCHS" = arm64 ] || fail "expected only arm64, found '$ARCHS'"
BUILD_INFO=$(xcrun vtool -show-build "$APP/$EXECUTABLE")
MACH_MIN=$(printf '%s\n' "$BUILD_INFO" | awk '$1 == "minos" {print $2}')
MACH_PLATFORM=$(printf '%s\n' "$BUILD_INFO" | awk '$1 == "platform" {print $2}')
[ "$MACH_PLATFORM" = IOS ] || fail "expected iOS device executable, found '$MACH_PLATFORM'"
[ "$MACH_MIN" = "$MINIMUM" ] || fail "Mach-O minimum '$MACH_MIN' differs from Info.plist '$MINIMUM'"

if [ -e "$APP/embedded.mobileprovision" ] && [ "$PRIVATE" = 0 ]; then
  fail "embedded provisioning profile from the build machine"
fi
if [ -e "$APP/PreparedResources/mobile-import.json" ] && [ "$PRIVATE" = 0 ]; then
  fail "embedded prepared Minecraft resources (use --private for a local build)"
fi

echo "App:      $APP"
echo "Bundle:   $BUNDLE $VERSION ($BUILD)"
echo "Minimum:  iOS $MINIMUM"
echo "Arch:     $ARCHS"
echo "Devices:  $FAMILY"
echo "Verified."
