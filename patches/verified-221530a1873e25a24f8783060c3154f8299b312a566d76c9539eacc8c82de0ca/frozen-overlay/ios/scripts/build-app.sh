#!/bin/sh
# Build the Horizon iOS app. No Apple account, certificate or team id is
# required: the default output is an unsigned .app that can be signed later.
#
#   ./ios/scripts/build-app.sh                                  # device, Release
#   ./ios/scripts/build-app.sh --platform simulator             # simulator, Release
#   DEVELOPMENT_TEAM=ABCDE12345 ./ios/scripts/build-app.sh --signed
#
# Environment: DEVELOPMENT_TEAM (only with --signed), HORIZON_DERIVED_DATA.
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
CONFIGURATION=Release
PLATFORM=device
DERIVED=${HORIZON_DERIVED_DATA:-}
SIGNED=0

usage() {
  sed -n '2,12p' "$0" | sed 's/^# \{0,1\}//'
  exit 2
}

while [ $# -gt 0 ]; do
  case "$1" in
    -c|--configuration) CONFIGURATION=$2; shift 2 ;;
    -p|--platform) PLATFORM=$2; shift 2 ;;
    -d|--derived-data) DERIVED=$2; shift 2 ;;
    --signed) SIGNED=1; shift ;;
    -h|--help) usage ;;
    *) echo "Unknown argument: $1" >&2; usage ;;
  esac
done

case "$PLATFORM" in
  device) SDK=iphoneos; DESTINATION='generic/platform=iOS'; PRODUCT_SUFFIX=-iphoneos ;;
  simulator) SDK=iphonesimulator; DESTINATION='generic/platform=iOS Simulator'; PRODUCT_SUFFIX=-iphonesimulator ;;
  *) echo "Platform must be device or simulator" >&2; exit 2 ;;
esac

DERIVED=${DERIVED:-"$ROOT/.local/xcode-$PLATFORM"}
mkdir -p "$DERIVED"

if [ "$SIGNED" = 1 ]; then
  : "${DEVELOPMENT_TEAM:?Set DEVELOPMENT_TEAM to your Apple team id for --signed}"
  SIGN_ARGS="DEVELOPMENT_TEAM=$DEVELOPMENT_TEAM CODE_SIGN_STYLE=Automatic"
else
  # Keep signing credentials outside this command and outside the repository.
  SIGN_ARGS="CODE_SIGNING_ALLOWED=NO CODE_SIGNING_REQUIRED=NO"
fi

MINIMUM=${IPHONEOS_DEPLOYMENT_TARGET:-$(awk '$1 == "IPHONEOS_DEPLOYMENT_TARGET" {print $3}' "$ROOT/ios/Deployment.xcconfig")}
echo "Building HorizonIOS ($CONFIGURATION, $PLATFORM) into $DERIVED"
# shellcheck disable=SC2086
xcodebuild -project "$ROOT/ios/HorizonIOS.xcodeproj" -scheme HorizonIOS \
  -configuration "$CONFIGURATION" -sdk "$SDK" -destination "$DESTINATION" \
  -derivedDataPath "$DERIVED" "IPHONEOS_DEPLOYMENT_TARGET=$MINIMUM" $SIGN_ARGS   ${HORIZON_BUNDLE_ID:+"PRODUCT_BUNDLE_IDENTIFIER=$HORIZON_BUNDLE_ID"} build

APP="$DERIVED/Build/Products/$CONFIGURATION$PRODUCT_SUFFIX/HorizonIOS.app"
if [ ! -d "$APP" ]; then
  echo "Build finished but $APP is missing" >&2
  exit 1
fi
echo "$APP"
