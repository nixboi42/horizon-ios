#import <XCTest/XCTest.h>

@interface NavigationKeyboardTests : XCTestCase
@property(nonatomic, strong) XCUIApplication *app;
@end
@implementation NavigationKeyboardTests
- (void)setUp {
    self.continueAfterFailure = NO;
    self.app = [[XCUIApplication alloc] init];
    self.app.launchEnvironment = @{@"HORIZON_IOS_TEST_SESSION": @"touch-lifecycle-ui-tests", @"HORIZON_NO_AUDIO": @"1"};
    [self.app launch];
    XCTAssertTrue([self.app.buttons[@"launcher.play"] waitForExistenceWithTimeout:120]);
}
- (void)startGame {
    [self.app.buttons[@"launcher.play"] tap];
    XCTAssertTrue([self.app.buttons[@"navigation.back"] waitForExistenceWithTimeout:120]);
    XCTAssertTrue([self.app.buttons[@"navigation.chat"] waitForExistenceWithTimeout:120]);
}
- (void)waitPlaying:(BOOL)playing {
    NSPredicate *predicate = [NSPredicate predicateWithFormat:@"exists == %@", @(playing)];
    XCTNSPredicateExpectation *expectation = [[XCTNSPredicateExpectation alloc] initWithPredicate:predicate object:self.app.buttons[@"navigation.chat"]];
    XCTAssertEqual([XCTWaiter waitForExpectations:@[expectation] timeout:10], XCTWaiterResultCompleted);
}
- (void)testRepeatedPauseResumeAndInventoryBack {
    [self startGame];
    for (NSUInteger i = 0; i < 3; ++i) {
        [self.app.buttons[@"navigation.back"] tap]; [self waitPlaying:NO];
        [self.app.buttons[@"navigation.back"] tap]; [self waitPlaying:YES];
    }
    [[self.app coordinateWithNormalizedOffset:CGVectorMake(0.82, 0.07)] tap];
    [self waitPlaying:NO];
    [self.app.buttons[@"navigation.back"] tap]; [self waitPlaying:YES];
}
- (void)testNativeChatSelectionDeletionAndKeyboardReopening {
    [self startGame];
    [self.app.buttons[@"navigation.chat"] tap];
    XCUIElement *text = self.app.textViews[@"horizon.nativeText"];
    XCTAssertTrue([text waitForExistenceWithTimeout:10]);
    XCTAssertTrue([self.app.keyboards.firstMatch waitForExistenceWithTimeout:10]);
    [text typeText:@"hello 123"];
    XCTAssertEqualObjects(text.value, @"hello 123");
    [text typeText:XCUIKeyboardKeyDelete];
    XCTAssertEqualObjects(text.value, @"hello 12");
    // Escape follows ChatScreen cancellation, then the same control pauses/resumes.
    [self.app.buttons[@"navigation.back"] tap]; [self waitPlaying:YES];
    XCTAssertFalse(text.exists);
    [self.app.buttons[@"navigation.chat"] tap];
    XCTAssertTrue([text waitForExistenceWithTimeout:10]);
    [text typeText:@"again"];
    XCTAssertEqualObjects(text.value, @"again");
    [self.app.buttons[@"navigation.back"] tap]; [self waitPlaying:YES];
}
- (void)testKeyboardFocusAfterBackgroundResume {
    [self startGame];
    [self.app.buttons[@"navigation.chat"] tap];
    XCUIElement *text = self.app.textViews[@"horizon.nativeText"];
    XCTAssertTrue([text waitForExistenceWithTimeout:10]);
    [text typeText:@"resume"];
    [XCUIDevice.sharedDevice pressButton:XCUIDeviceButtonHome];
    [self.app activate];
    XCTAssertTrue([text waitForExistenceWithTimeout:10]);
    XCTAssertEqualObjects(text.value, @"resume");
    [text typeText:@"d"];
    XCTAssertEqualObjects(text.value, @"resumed");
    [self.app.buttons[@"navigation.back"] tap]; [self waitPlaying:YES];
}
- (void)testRepeatedPlayReturnAndSavedUsername {
    [self.app.buttons[@"launcher.settings"] tap];
    XCUIElement *username = self.app.textFields[@"launcher.username"];
    XCTAssertTrue([username waitForExistenceWithTimeout:10]);
    [username tap];
    [username pressForDuration:1.1];
    XCUIElement *selectAll = self.app.menuItems[@"Select All"];
    if ([selectAll waitForExistenceWithTimeout:3]) { [selectAll tap]; }
    [username typeText:@"CycleName"];
    XCTAssertEqualObjects(username.value, @"CycleName");
    [self.app.buttons[@"launcher.done"] tap];

    for (NSUInteger i = 0; i < 2; ++i) {
        [self startGame];
        [self.app.buttons[@"launcher.return"] tap];
        XCTAssertTrue([self.app.buttons[@"launcher.play"] waitForExistenceWithTimeout:120]);
    }

    [self.app terminate];
    [self.app launch];
    XCTAssertTrue([self.app.buttons[@"launcher.settings"] waitForExistenceWithTimeout:120]);
    [self.app.buttons[@"launcher.settings"] tap];
    username = self.app.textFields[@"launcher.username"];
    XCTAssertTrue([username waitForExistenceWithTimeout:10]);
    XCTAssertEqualObjects(username.value, @"CycleName");
}
@end
