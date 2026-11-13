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
    XCTAssertTrue([self.app.buttons[@"navigation.back"] waitForExistenceWithTimeout:120]);
    XCTAssertTrue([self.app.buttons[@"navigation.chat"] waitForExistenceWithTimeout:120]);
}
- (void)waitPlaying:(BOOL)playing {
    NSPredicate *predicate = [NSPredicate predicateWithFormat:@"exists == %@", @(playing)];
    XCTNSPredicateExpectation *expectation = [[XCTNSPredicateExpectation alloc] initWithPredicate:predicate object:self.app.buttons[@"navigation.chat"]];
    XCTAssertEqual([XCTWaiter waitForExpectations:@[expectation] timeout:10], XCTWaiterResultCompleted);
}
- (void)testRepeatedPauseResumeAndInventoryBack {
    for (NSUInteger i = 0; i < 3; ++i) {
        [self.app.buttons[@"navigation.back"] tap]; [self waitPlaying:NO];
        [self.app.buttons[@"navigation.back"] tap]; [self waitPlaying:YES];
    }
    [[self.app coordinateWithNormalizedOffset:CGVectorMake(0.82, 0.07)] tap];
    [self waitPlaying:NO];
    [self.app.buttons[@"navigation.back"] tap]; [self waitPlaying:YES];
}
- (void)testNativeChatSelectionDeletionAndKeyboardReopening {
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
@end
