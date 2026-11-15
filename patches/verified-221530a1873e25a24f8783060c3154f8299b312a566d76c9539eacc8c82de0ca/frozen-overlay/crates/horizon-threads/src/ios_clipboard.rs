//! Public UIKit text clipboard. Call from a user-initiated paste/copy action;
//! reading the pasteboard may prompt the user on recent iOS versions.
use objc2_foundation::{NSString, NSThread};
use objc2_ui_kit::UIPasteboard;

/// `None` means UIKit cannot be used from this thread; an empty pasteboard is
/// `Some("")`, so callers do not paste stale in-process text in its place.
pub fn get() -> Option<String> {
    if !NSThread::isMainThread_class() {
        return None;
    }
    // SAFETY: UIKit access is on the main thread; retained objects remain alive
    // until their contents have been copied to owned Rust text.
    Some(
        unsafe { UIPasteboard::generalPasteboard().string() }
            .map_or_else(String::new, |s| s.to_string()),
    )
}

/// Returns false when UIKit cannot be used from the calling thread.
pub fn set(text: &str) -> bool {
    if !NSThread::isMainThread_class() {
        return false;
    }
    // SAFETY: main thread, live NSString; UIKit copies the value.
    unsafe {
        UIPasteboard::generalPasteboard().setString(Some(&NSString::from_str(text)));
    }
    true
}

/// Ask the iOS host to close the active game session.
pub fn return_to_launcher() {
    unsafe extern "C" {
        fn horizon_ios_return_to_launcher();
    }
    // SAFETY: the host callback takes no arguments and queues a session command.
    unsafe { horizon_ios_return_to_launcher() };
}
