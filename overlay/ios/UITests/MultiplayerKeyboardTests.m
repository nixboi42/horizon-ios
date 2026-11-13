#import <XCTest/XCTest.h>
#import <UIKit/UIKit.h>

@interface MultiplayerKeyboardTests : XCTestCase
@property(nonatomic, strong) XCUIApplication *app;
@property(nonatomic, copy) NSString *clipboardFixture;
@property(nonatomic) CGFloat guiWidth;
@property(nonatomic) CGFloat guiHeight;
@end
@implementation MultiplayerKeyboardTests
- (void)setUp {
    self.continueAfterFailure = NO;
    self.app = [[XCUIApplication alloc] init];
    self.app.launchEnvironment = @{@"HORIZON_IOS_TEST_SESSION": [@"phase41-extra-" stringByAppendingString:NSUUID.UUID.UUIDString], @"HORIZON_NO_AUDIO": @"1"};
    self.app.launchArguments = @[@"-AppleLanguages", @"(en)", @"-AppleLocale", @"en_US"];
    self.clipboardFixture = [self.name containsString:@"Unicode"] ? @"İıŞşĞğÇçÖöÜü😀e\u0301" : @"[::1]:25568";
    // Recent iOS versions deny pasteboard writes by a background test runner.
    XCUIApplication *runner = [[XCUIApplication alloc] initWithBundleIdentifier:NSBundle.mainBundle.bundleIdentifier];
    [runner activate];
    UIPasteboard.generalPasteboard.string = self.clipboardFixture;
    [self.app launch];
    XCTAssertTrue([self.app.buttons[@"navigation.chat"] waitForExistenceWithTimeout:120]);
    CGImageRef image = self.app.screenshot.image.CGImage;
    CGFloat width = MAX(CGImageGetWidth(image), CGImageGetHeight(image));
    CGFloat height = MIN(CGImageGetWidth(image), CGImageGetHeight(image));
    CGFloat scale = floor(MIN(width / 320, height / 240));
    self.guiWidth = ceil(width / scale); self.guiHeight = ceil(height / scale);
}
- (void)capture:(NSString *)name {
    XCTAttachment *attachment = [XCTAttachment attachmentWithScreenshot:self.app.screenshot];
    attachment.name = name; attachment.lifetime = XCTAttachmentLifetimeKeepAlways;
    [self addAttachment:attachment];
}
- (void)tapGUIX:(CGFloat)x y:(CGFloat)y {
    [[self.app coordinateWithNormalizedOffset:CGVectorMake(x / self.guiWidth, y / self.guiHeight)] tap];
}
- (XCUIElement *)text {
    XCUIElement *text = self.app.textViews[@"horizon.nativeText"];
    XCTAssertTrue([text waitForExistenceWithTimeout:10]);
    XCTAssertTrue(text.hittable, @"The mirrored field must be visible and touch-selectable");
    return text;
}
- (void)expectText:(NSString *)value {
    NSPredicate *predicate = [NSPredicate predicateWithFormat:@"value == %@", value];
    XCTNSPredicateExpectation *expectation = [[XCTNSPredicateExpectation alloc] initWithPredicate:predicate object:self.text];
    XCTAssertEqual([XCTWaiter waitForExpectations:@[expectation] timeout:5], XCTWaiterResultCompleted);
}
- (XCUIElement *)editMenu:(NSString *)title {
    XCUIElement *item = self.app.menuItems[title];
    if ([item waitForExistenceWithTimeout:2]) { return item; }
    item = self.app.buttons[title];
    XCTAssertTrue([item waitForExistenceWithTimeout:3]);
    return item;
}
- (void)paste:(NSString *)value openMenu:(BOOL)openMenu {
    XCTAssertEqualObjects(value, self.clipboardFixture);
    // Seed immediately before Paste, not before a potentially long world load.
    XCUIApplication *runner = [[XCUIApplication alloc] initWithBundleIdentifier:NSBundle.mainBundle.bundleIdentifier];
    [runner activate];
    UIPasteboard.generalPasteboard.string = value;
    XCTAssertEqualObjects(UIPasteboard.generalPasteboard.string, value);
    [self.app activate];
    [self.text pressForDuration:1.2];
    if (!openMenu && self.app.menuItems[@"Select All"].exists) { [self.app.menuItems[@"Select All"] tap]; }
    [[self editMenu:@"Paste"] tap];
    XCUIApplication *springboard = [[XCUIApplication alloc] initWithBundleIdentifier:@"com.apple.springboard"];
    if (springboard.alerts.firstMatch.exists) {
        XCUIElement *allow = [springboard.alerts.buttons matchingPredicate:[NSPredicate predicateWithFormat:@"label == 'Allow Paste' OR label == 'Einsetzen erlauben' OR label == 'Einfügen erlauben' OR label == 'Erlauben'"]].firstMatch;
        if (allow.exists) { [allow tap]; }
    }
}
- (void)hideKeyboard { [self.app.toolbars.buttons.allElementsBoundByIndex.lastObject tap]; }
- (void)testUnicodePasteCursorDeletionAndReplacement {
    [self.app.buttons[@"navigation.chat"] tap];
    XCTAssertTrue([self.app.keyboards.firstMatch waitForExistenceWithTimeout:10]);
    [self paste:@"İıŞşĞğÇçÖöÜü😀e\u0301" openMenu:YES];
    [self expectText:@"İıŞşĞğÇçÖöÜü😀e\u0301"];
    [self.text typeKey:XCUIKeyboardKeyLeftArrow modifierFlags:XCUIKeyModifierNone];
    [self.text typeText:XCUIKeyboardKeyDelete];
    [self.text typeText:@"X"];
    [self expectText:@"İıŞşĞğÇçÖöÜüXe\u0301"];
    [self.text pressForDuration:1.2];
    [[self editMenu:@"Select All"] tap];
    [self.text typeText:@"replaced"];
    [self expectText:@"replaced"];
    [self capture:@"Unicode paste and native selection replacement"];
    [self.app.buttons[@"navigation.back"] tap];
    XCTAssertTrue([self.app.buttons[@"navigation.chat"] waitForExistenceWithTimeout:10]);
}
- (void)openMultiplayer {
    [self.app.buttons[@"navigation.back"] tap];
    CGFloat y0 = floor((self.guiHeight - 166) * 0.25);
    [self tapGUIX:self.guiWidth / 2 y:y0 + 156]; // Save and Quit through the real pause UI.
    [self capture:@"Title after saving disposable world"];
    [self tapGUIX:self.guiWidth / 2 y:floor(self.guiHeight / 4) + 82];
    [self capture:@"Multiplayer warning or server list"];
    // First visit's Proceed; on later visits this lies harmlessly inside the list.
    [self tapGUIX:floor(self.guiWidth / 2) - 79 y:self.guiHeight / 2 + 59];
}
- (void)testAddEditDirectAddressPersistenceAndBack {
    [self openMultiplayer];
    CGFloat cx = floor(self.guiWidth / 2);
    CGFloat listX = floor((self.guiWidth - 308) / 2);
    [self tapGUIX:listX + 258 y:self.guiHeight - 42]; // Add Server.
    [self hideKeyboard];
    [self tapGUIX:cx y:76]; // Explicitly select Server Name; Add initially focuses Address.
    [self.text typeText:@" Phase41"];
    NSString *name = [self.text.value copy];
    [self hideKeyboard];
    [self tapGUIX:cx y:116]; // Existing Server Address field.
    [self.text typeText:@"127.0.0.1:25566"];
    [self expectText:@"127.0.0.1:25566"];
    [self hideKeyboard];
    [self tapGUIX:cx y:floor(self.guiHeight / 4) + 124]; // Existing Done saves servers.dat.
    [self capture:@"Saved server list"];
    [self tapGUIX:cx y:50];
    [self tapGUIX:listX + 37 y:self.guiHeight - 18]; // Edit the saved server.
    [self hideKeyboard]; [self tapGUIX:cx y:76];
    [self expectText:name];
    [self hideKeyboard]; [self tapGUIX:cx y:116];
    [self expectText:@"127.0.0.1:25566"];
    [self.text pressForDuration:1.2]; [[self editMenu:@"Select All"] tap];
    [self paste:@"[::1]:25568" openMenu:NO];
    [self expectText:@"[::1]:25568"];
    [self hideKeyboard]; [self tapGUIX:cx y:floor(self.guiHeight / 4) + 124];
    [self tapGUIX:listX + 154 y:self.guiHeight - 42]; // Direct Connection.
    if ([(NSString *)self.text.value length]) {
        [self.text pressForDuration:1.2]; [[self editMenu:@"Select All"] tap];
        [self.text typeText:XCUIKeyboardKeyDelete];
    }
    [self.text typeText:@"mc.example.invalid:25567"];
    [self expectText:@"mc.example.invalid:25567"];
    [self.app.buttons[@"navigation.back"] tap]; // Direct -> list, not title.
    [self tapGUIX:cx y:50];
    [self tapGUIX:listX + 37 y:self.guiHeight - 18];
    [self hideKeyboard]; [self tapGUIX:cx y:76];
    [self expectText:name];
    [self hideKeyboard]; [self tapGUIX:cx y:116];
    [self expectText:@"[::1]:25568"];
    [self capture:@"Saved bracketed IPv6 address reopened for editing"];
    [self.app.buttons[@"navigation.back"] tap]; // Edit -> list.
    [self.app.buttons[@"navigation.back"] tap]; // List -> title.
    [self capture:@"Native Back returned to title"];
}
@end

// Temporary device driver; removed after this investigation.
@interface PacketJoinDeviceTests : MultiplayerKeyboardTests @end
@implementation PacketJoinDeviceTests
- (void)setUp {
    self.continueAfterFailure = NO;
    self.app = [[XCUIApplication alloc] init];
    self.app.launchEnvironment = @{@"HORIZON_IOS_TEST_SESSION": [@"packet-join-" stringByAppendingString:NSUUID.UUID.UUIDString], @"HORIZON_PACKET_DIAGNOSTICS": @"1", @"HORIZON_NO_AUDIO": @"1"};
    self.app.launchArguments = @[@"-AppleLanguages", @"(en)", @"-AppleLocale", @"en_US"];
    [self.app launch];
    XCTAssertTrue([self.app.buttons[@"navigation.chat"] waitForExistenceWithTimeout:120]);
    CGImageRef image = self.app.screenshot.image.CGImage;
    CGFloat width = MAX(CGImageGetWidth(image), CGImageGetHeight(image));
    CGFloat height = MIN(CGImageGetWidth(image), CGImageGetHeight(image));
    CGFloat scale = floor(MIN(width / 320, height / 240));
    self.guiWidth = ceil(width / scale); self.guiHeight = ceil(height / scale);
}
- (void)testJoinServer {
    [self openMultiplayer];
    CGFloat cx = floor(self.guiWidth / 2);
    CGFloat listX = floor((self.guiWidth - 308) / 2);
    [self tapGUIX:listX + 154 y:self.guiHeight - 42];
    [self.text typeText:@"192.168.1.18:25595"];
    [self hideKeyboard];
    [self tapGUIX:cx y:floor(self.guiHeight / 4) + 124];
    XCTAssertTrue([self.app.buttons[@"navigation.chat"] waitForExistenceWithTimeout:90]);
    [self capture:@"iPhone joined corrected server"];
    [self.app.buttons[@"navigation.chat"] tap];
    [self.text typeText:@"protocol device test\n"];
    if (self.app.buttons[@"navigation.back"].exists && self.text.exists) { [self.app.buttons[@"navigation.back"] tap]; }
    [NSThread sleepForTimeInterval:20];
    XCTAssertTrue(self.app.buttons[@"navigation.chat"].exists);
    [self capture:@"iPhone remained connected"];
}
@end
