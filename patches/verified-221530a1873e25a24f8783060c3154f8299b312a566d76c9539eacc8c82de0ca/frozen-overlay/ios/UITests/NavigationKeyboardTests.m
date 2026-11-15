#import <XCTest/XCTest.h>

@interface NavigationKeyboardTests : XCTestCase
@property(nonatomic, strong) XCUIApplication *app;
@end
@implementation NavigationKeyboardTests
- (void)setUp {
    self.continueAfterFailure = NO;
    self.app = [[XCUIApplication alloc] init];
    self.app.launchEnvironment = @{
        @"HORIZON_IOS_TEST_SESSION": [@"touch-lifecycle-" stringByAppendingString:NSUUID.UUID.UUIDString],
        @"HORIZON_NO_AUDIO": @"1"
    };
    self.app.launchArguments = @[@"-AppleLanguages", @"(en)", @"-AppleLocale", @"en_US"];
    [self.app launch];
    [self waitForPlay];
}
- (void)waitForPlay {
    XCUIElement *play = self.app.buttons[@"launcher.play"];
    NSPredicate *ready = [NSPredicate predicateWithFormat:@"exists == YES AND enabled == YES"];
    XCTNSPredicateExpectation *expectation = [[XCTNSPredicateExpectation alloc] initWithPredicate:ready object:play];
    XCTAssertEqual([XCTWaiter waitForExpectations:@[expectation] timeout:120], XCTWaiterResultCompleted);
}
- (void)openTitle {
    [self.app.buttons[@"launcher.play"] tap];
    XCTAssertTrue([self.app.buttons[@"navigation.back"] waitForExistenceWithTimeout:120]);
    XCTAssertFalse(self.app.buttons[@"launcher.play"].exists);
    XCTAssertFalse(self.app.buttons[@"navigation.chat"].exists);
}
- (void)openSingleplayer:(CGSize)size {
    [self tapGameX:floor(size.width / 2) y:floor(size.height / 4) + 58 size:size];
}
- (void)startGame {
    [self openTitle];
    CGSize size = [self gameSize];
    [self openSingleplayer:size]; // A fresh instance opens Create New World.
    CGFloat modeY = 24 + trunc((size.height - 174) / 6) + 51;
    [self tapGameX:floor(size.width / 2) y:modeY size:size]; // Survival → Hardcore
    [self tapGameX:floor(size.width / 2) y:modeY size:size]; // Hardcore → Creative
    [self tapGameX:floor(size.width / 2) - 79 y:size.height - 17 size:size];
    XCTAssertTrue([self.app.buttons[@"navigation.chat"] waitForExistenceWithTimeout:120]);
}
- (void)waitPlaying:(BOOL)playing {
    NSPredicate *predicate = [NSPredicate predicateWithFormat:@"exists == %@", @(playing)];
    XCTNSPredicateExpectation *expectation = [[XCTNSPredicateExpectation alloc] initWithPredicate:predicate object:self.app.buttons[@"navigation.chat"]];
    XCTAssertEqual([XCTWaiter waitForExpectations:@[expectation] timeout:10], XCTWaiterResultCompleted);
}
- (void)expectKeyboardClosed {
    // Inverted wait also catches a late responder request on a subsequent frame.
    XCUIElement *keyboard = self.app.keyboards.firstMatch;
    XCTNSPredicateExpectation *reopened = [[XCTNSPredicateExpectation alloc]
        initWithPredicate:[NSPredicate predicateWithFormat:@"exists == YES"] object:keyboard];
    reopened.inverted = YES;
    XCTAssertEqual([XCTWaiter waitForExpectations:@[reopened] timeout:1], XCTWaiterResultCompleted);
    XCTAssertFalse(self.app.textViews[@"horizon.nativeText"].exists);
}
- (void)waitForKeyboardToClose {
    XCTNSPredicateExpectation *closed = [[XCTNSPredicateExpectation alloc]
        initWithPredicate:[NSPredicate predicateWithFormat:@"exists == NO"] object:self.app.keyboards.firstMatch];
    XCTAssertEqual([XCTWaiter waitForExpectations:@[closed] timeout:10], XCTWaiterResultCompleted);
    [self expectKeyboardClosed];
}
- (CGSize)gameSize {
    CGImageRef pixels = self.app.screenshot.image.CGImage;
    CGFloat w = MAX(CGImageGetWidth(pixels), CGImageGetHeight(pixels));
    CGFloat h = MIN(CGImageGetWidth(pixels), CGImageGetHeight(pixels));
    CGFloat scale = MAX(1, floor(MIN(w / 320, h / 240)));
    return CGSizeMake(ceil(w / scale), ceil(h / scale));
}
- (void)tapGameX:(CGFloat)x y:(CGFloat)y size:(CGSize)size {
    [[self.app coordinateWithNormalizedOffset:CGVectorMake(x / size.width, y / size.height)] tap];
}
- (void)quitFromTitle {
    CGSize size = [self gameSize];
    [self tapGameX:floor(size.width / 2) + 51 y:floor(size.height / 4) + 154 size:size];
    [self waitForPlay];
    XCTAssertFalse(self.app.buttons[@"navigation.back"].exists);
}

// Sample the launcher's background away from its controls. Accessibility
// alone can see the launcher even when a Metal layer still covers it.
- (NSArray<NSNumber *> *)launcherBackground {
    XCUIScreenshot *screenshot = self.app.screenshot;
    CGImageRef image = screenshot.image.CGImage;
    CGRect sample = CGRectMake(floor(CGImageGetWidth(image) * 0.15),
                               floor(CGImageGetHeight(image) * 0.50), 1, 1);
    CGImageRef cropped = CGImageCreateWithImageInRect(image, sample);
    uint8_t rgba[4] = {0};
    CGColorSpaceRef space = CGColorSpaceCreateDeviceRGB();
    CGContextRef context = CGBitmapContextCreate(rgba, 1, 1, 8, 4, space,
                                                 kCGImageAlphaPremultipliedLast);
    CGContextDrawImage(context, CGRectMake(0, 0, 1, 1), cropped);
    CGContextRelease(context);
    CGColorSpaceRelease(space);
    CGImageRelease(cropped);
    return @[@(rgba[0]), @(rgba[1]), @(rgba[2])];
}

- (void)testQuitGameWithoutOpeningAWorld {
    NSArray<NSNumber *> *background = [self launcherBackground];
    for (NSUInteger i = 0; i < 2; ++i) {
        [self openTitle];
        [self quitFromTitle];
        NSArray<NSNumber *> *restored = [self launcherBackground];
        for (NSUInteger channel = 0; channel < background.count; ++channel) {
            XCTAssertEqualWithAccuracy(restored[channel].intValue, background[channel].intValue, 5);
        }
    }
}

- (void)testQuitGameAfterSavingAndReturningToTitle {
    [self startGame];
    CGSize size = [self gameSize];
    CGFloat cx = floor(size.width / 2);
    [self.app.buttons[@"navigation.back"] tap];
    [self tapGameX:cx y:floor((size.height - 166) * 0.25) + 156 size:size];
    [self waitPlaying:NO];
    [self quitFromTitle];

    [self openTitle];
    [self openSingleplayer:size];
    [self tapGameX:cx y:66 size:size];
    [self tapGameX:cx - 79 y:size.height - 42 size:size];
    XCTAssertTrue([self.app.buttons[@"navigation.chat"] waitForExistenceWithTimeout:120]);
    [self.app.buttons[@"launcher.return"] tap];
    [self waitForPlay];
}

- (void)testWorldListAndCreationRequireFieldTaps {
    [self startGame];
    CGSize size = [self gameSize];
    CGFloat cx = floor(size.width / 2);
    [self.app.buttons[@"navigation.back"] tap];
    [self tapGameX:cx y:floor((size.height - 166) * 0.25) + 156 size:size]; // Save and Quit
    [self openSingleplayer:size];
    [self expectKeyboardClosed];
    [self.app typeKey:@"q" modifierFlags:XCUIKeyModifierNone];
    [self expectKeyboardClosed];
    [self tapGameX:cx y:30 size:size]; // The world search was already focused.
    XCTAssertTrue([self.app.keyboards.firstMatch waitForExistenceWithTimeout:10]);
    XCTAssertEqualObjects(self.app.textViews[@"horizon.nativeText"].value, @"q");
    [self.app.toolbars.buttons[@"Done"] tap];
    [self waitForKeyboardToClose];
    [self tapGameX:cx y:30 size:size];
    XCTAssertTrue([self.app.keyboards.firstMatch waitForExistenceWithTimeout:10]);
    [self.app.toolbars.buttons[@"Done"] tap];
    [self waitForKeyboardToClose];
    [self tapGameX:cx y:48 size:size]; // Outside the search field.
    [self expectKeyboardClosed];
    [self tapGameX:cx + 75 y:size.height - 42 size:size]; // Create New World
    [self expectKeyboardClosed];
    CGFloat nameY = 24 + trunc((size.height - 174) / 6) + 23;
    [self tapGameX:cx y:nameY size:size]; // The new world's initially focused name.
    XCTAssertTrue([self.app.keyboards.firstMatch waitForExistenceWithTimeout:15]);
    XCUIElement *text = self.app.textViews[@"horizon.nativeText"];
    XCTAssertTrue([text waitForExistenceWithTimeout:10]);
    XCTAssertTrue([(NSString *)text.value length] > 0);
    [self.app.buttons[@"navigation.back"] tap];
    [self waitForKeyboardToClose]; // Back returns to the world list and focuses its search.
}
- (void)runGameCommand:(NSString *)command {
    [self.app.buttons[@"navigation.chat"] tap];
    XCUIElement *text = self.app.textViews[@"horizon.nativeText"];
    XCTAssertTrue([text waitForExistenceWithTimeout:10]);
    [text typeText:[command stringByAppendingString:@"\n"]];
    [self waitPlaying:YES];
}
- (void)testSleepingRequiresExplicitChatFieldTap {
    [self startGame];
    [self runGameCommand:@"/tp @s 0.5 200 0.5 0 55"];
    [self runGameCommand:@"/fill -2 199 -2 2 199 3 minecraft:stone"];
    [self runGameCommand:@"/setblock 0 200 2 minecraft:red_bed[facing=north,part=foot] strict"];
    [self runGameCommand:@"/setblock 0 200 1 minecraft:red_bed[facing=north,part=head] strict"];
    [self runGameCommand:@"/gamerule minecraft:players_sleeping_percentage 101"];
    [self runGameCommand:@"/time set midnight"];
    [self runGameCommand:@"/tp @s 0.5 200 0.5 0 55"];
    [[self.app coordinateWithNormalizedOffset:CGVectorMake(0.93, 0.90)] tap]; // Use the bed
    [self waitPlaying:NO];
    [self expectKeyboardClosed];
    CGSize size = [self gameSize];
    // The keyboard may appear here because the sleeping chat input was tapped.
    [self tapGameX:floor(size.width * 0.30) y:size.height - 6 size:size];
    XCTAssertTrue([self.app.keyboards.firstMatch waitForExistenceWithTimeout:10]);
    [self.app.buttons[@"navigation.back"] tap]; // Leave bed
    [self waitPlaying:YES];
    [self waitForKeyboardToClose];
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
    [self waitForKeyboardToClose];
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
    [self waitForKeyboardToClose];
    CGSize size = [self gameSize];
    [self tapGameX:floor(size.width * 0.30) y:size.height - 6 size:size];
    XCTAssertTrue([self.app.keyboards.firstMatch waitForExistenceWithTimeout:10]);
    XCTAssertTrue([text waitForExistenceWithTimeout:10]);
    XCTAssertEqualObjects(text.value, @"resume");
    [text typeText:@"d"];
    XCTAssertEqualObjects(text.value, @"resumed");
    [self.app.buttons[@"navigation.back"] tap]; [self waitPlaying:YES];
}
- (void)manage:(NSString *)action {
    [self.app.buttons[@"launcher.manage"] tap];
    [self.app.buttons[action] tap];
}
- (void)saveInstanceName:(NSString *)suffix {
    XCUIElement *field = self.app.textFields[@"launcher.instance_name"];
    XCTAssertTrue([field waitForExistenceWithTimeout:10]);
    [field tap];
    if ([field.value isEqual:@"Sandbox"]) {
        [field pressForDuration:1.0];
        XCUIElement *selectAll = self.app.menuItems[@"Select All"];
        if ([selectAll waitForExistenceWithTimeout:2]) { [selectAll tap]; }
    }
    [field typeText:suffix];
    [self.app.buttons[@"launcher.instance_save"] tap];
    [self waitForPlay];
}
- (void)testInstanceActionsAndPersistentSelection {
    // Give this case its own catalog. Like the other navigation cases, the
    // app uses the test container's prepared resources.
    [self.app terminate];
    self.app.launchEnvironment = @{
        @"HORIZON_IOS_TEST_SESSION": [@"instances-" stringByAppendingString:NSUUID.UUID.UUIDString]
    };
    [self.app launch];
    [self waitForPlay];

    [self manage:@"launcher.create"];
    [self saveInstanceName:@"Sandbox"];
    [self manage:@"launcher.rename"];
    [self saveInstanceName:@"Renamed"];
    [self manage:@"launcher.duplicate"];
    [self saveInstanceName:@"Copy"];

    [self.app.buttons[@"launcher.instance"] tap];
    [self.app.buttons[@"launcher.select.Renamed"] tap];
    [self waitForPlay];
    [self.app terminate];
    [self.app launch];
    [self waitForPlay];
    XCTAssertTrue([self.app.buttons[@"launcher.instance"].label containsString:@"Renamed"]);

    [self openTitle];
    [self.app.buttons[@"launcher.return"] tap];
    [self waitForPlay];
    [self manage:@"launcher.delete"];
    XCUIElement *confirmDelete = self.app.alerts.firstMatch.buttons[@"launcher.confirm_delete"].firstMatch;
    XCTAssertTrue([confirmDelete waitForExistenceWithTimeout:10]);
    [confirmDelete tap];
    [self waitForPlay];
    [self.app.buttons[@"launcher.instance"] tap];
    XCTAssertFalse(self.app.buttons[@"launcher.select.Renamed"].exists);
    XCTAssertTrue(self.app.buttons[@"launcher.select.Copy"].exists);
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

    [self startGame];
    [self.app.buttons[@"navigation.chat"] tap];
    XCTAssertTrue([self.app.keyboards.firstMatch waitForExistenceWithTimeout:10]);
    [self.app.buttons[@"launcher.return"] tap];
    [self waitForPlay];
    [self waitForKeyboardToClose];

    // This instance now has a save. Play must still open the title every time.
    for (NSUInteger i = 0; i < 2; ++i) {
        [self openTitle];
        [self expectKeyboardClosed];
        [self quitFromTitle];
    }

    [self openTitle];
    CGSize size = [self gameSize];
    [self openSingleplayer:size];
    // Select the saved world and open it through the world list.
    [self tapGameX:floor(size.width / 2) y:66 size:size];
    [self tapGameX:floor(size.width / 2) - 79 y:size.height - 42 size:size];
    XCTAssertTrue([self.app.buttons[@"navigation.chat"] waitForExistenceWithTimeout:120]);
    [self.app.buttons[@"navigation.back"] tap];
    [self tapGameX:floor(size.width / 2) y:floor((size.height - 166) * 0.25) + 156 size:size];
    [self quitFromTitle];

    [self.app terminate];
    [self.app launch];
    XCTAssertTrue([self.app.buttons[@"launcher.settings"] waitForExistenceWithTimeout:120]);
    [self.app.buttons[@"launcher.settings"] tap];
    username = self.app.textFields[@"launcher.username"];
    XCTAssertTrue([username waitForExistenceWithTimeout:10]);
    XCTAssertEqualObjects(username.value, @"CycleName");
}
@end
