// winit calls UIApplicationMain and owns the app delegate/view controller.
// Do not add a second UIKit or MTKView render loop here.
#import <UIKit/UIKit.h>

// iOS 27 requires scene adoption. winit 0.30 still creates the UIWindow and
// view controller; this delegate only supplies its public UIWindowScene.
static UIWindowScene *horizonScene;
static UIWindow *horizonWindow;
static NSString *horizonStatus;
void horizon_ios_status(const char *key);
void horizon_ios_input_state(const char *json);
static void attachWindow(void) {
    if (!horizonWindow || !horizonScene) { return; }
    horizonWindow.windowScene = horizonScene;
    horizonWindow.frame = horizonScene.coordinateSpace.bounds;
    [horizonWindow makeKeyAndVisible];
    [horizonWindow layoutIfNeeded];
    NSLog(@"Horizon scene attached: %@", NSStringFromCGRect(horizonWindow.bounds));
    if (horizonStatus) { horizon_ios_status(horizonStatus.UTF8String); }
}
@interface HorizonSceneDelegate : UIResponder <UIWindowSceneDelegate>
@end
@implementation HorizonSceneDelegate
- (void)scene:(UIScene *)scene willConnectToSession:(UISceneSession *)session options:(UISceneConnectionOptions *)options {
    if ([scene isKindOfClass:UIWindowScene.class]) { horizonScene = (UIWindowScene *)scene; attachWindow(); }
}
- (void)sceneWillResignActive:(UIScene *)scene { horizon_ios_input_state("null"); }
- (void)sceneDidDisconnect:(UIScene *)scene {
    if (horizonScene == scene) { horizonScene = nil; }
}
@end

void horizon_ios_bind_view(void *rawView) {
    NSCAssert([NSThread isMainThread], @"UI must be on the main thread");
    UIView *view = (__bridge UIView *)rawView;
    horizonWindow = view.window;
    attachWindow();
}

double horizon_ios_safe_area_bottom(void *rawView) {
    NSCAssert([NSThread isMainThread], @"UI must be on the main thread");
    return ((__bridge UIView *)rawView).safeAreaInsets.bottom;
}

extern void horizon_ios_run(const char *support, const char *cache, const char *bundle, const char *documents);

// Supported UIKit background time for the server's asynchronous save/flush.
static UIBackgroundTaskIdentifier horizonSaveTask;
static BOOL horizonSaveTaskInitialized;
void horizon_ios_background_save_begin(void) {
    if (!horizonSaveTaskInitialized) { horizonSaveTask = UIBackgroundTaskInvalid; horizonSaveTaskInitialized = YES; }
    if (horizonSaveTask != UIBackgroundTaskInvalid) { return; }
    horizonSaveTask = [UIApplication.sharedApplication beginBackgroundTaskWithName:@"Horizon world save" expirationHandler:^{
        NSLog(@"Horizon background save time expired; completion is unconfirmed");
        [UIApplication.sharedApplication endBackgroundTask:horizonSaveTask];
        horizonSaveTask = UIBackgroundTaskInvalid;
    }];
}
void horizon_ios_background_save_end(void) {
    if (!horizonSaveTaskInitialized) { return; }
    if (horizonSaveTask != UIBackgroundTaskInvalid) {
        [UIApplication.sharedApplication endBackgroundTask:horizonSaveTask];
        horizonSaveTask = UIBackgroundTaskInvalid;
    }
}

extern void horizon_ios_sensitivity(float value);
@interface HorizonFlightControls : UIView
@property(nonatomic, strong) UILabel *move;
@property(nonatomic, strong) UILabel *up;
@property(nonatomic, strong) UILabel *down;
@property(nonatomic, strong) UILabel *sensitivityLabel;
@property(nonatomic, strong) UISlider *sensitivity;
@end
@implementation HorizonFlightControls
- (instancetype)initWithFrame:(CGRect)frame {
    if ((self = [super initWithFrame:frame])) {
        self.userInteractionEnabled = YES;
        BOOL gameplay = [horizonStatus hasPrefix:@"gameplay."];
        NSArray *keys = gameplay ? @[@"gameplay.move", @"gameplay.jump", @"gameplay.break", @"gameplay.place", @"gameplay.pick", @"gameplay.inventory"] : @[@"flight.move", @"flight.up", @"flight.down", @"flight.sensitivity"];
        NSMutableArray *labels = [NSMutableArray new];
        for (NSString *key in keys) {
            UILabel *label = [UILabel new];
            label.text = NSLocalizedString(key, nil);
            label.textColor = UIColor.whiteColor;
            label.backgroundColor = [UIColor colorWithWhite:0 alpha:0.35];
            label.textAlignment = NSTextAlignmentCenter;
            label.numberOfLines = 0;
            label.font = [UIFont preferredFontForTextStyle:UIFontTextStyleCaption1];
            [self addSubview:label];
            [labels addObject:label];
        }
        if (gameplay) { self.sensitivity = nil; return self; }
        self.move = labels[0]; self.up = labels[1]; self.down = labels[2]; self.sensitivityLabel = labels[3];
        self.sensitivity = [UISlider new];
        self.sensitivity.minimumValue = 0.03; self.sensitivity.maximumValue = 0.6; self.sensitivity.value = 0.18;
        self.sensitivity.accessibilityLabel = NSLocalizedString(@"flight.sensitivity", nil);
        [self.sensitivity addTarget:self action:@selector(changeSensitivity:) forControlEvents:UIControlEventValueChanged];
        [self addSubview:self.sensitivity];
    }
    return self;
}
- (void)changeSensitivity:(UISlider *)sender { horizon_ios_sensitivity(sender.value); }
- (UIView *)hitTest:(CGPoint)point withEvent:(UIEvent *)event {
    // Let winit receive every flight touch except the actual native slider.
    if (self.hidden || !self.userInteractionEnabled || self.alpha <= 0.01) { return nil; }
    if (!self.sensitivity) { return nil; }
    CGPoint sliderPoint = [self.sensitivity convertPoint:point fromView:self];
    return [self.sensitivity pointInside:sliderPoint withEvent:event] ? [self.sensitivity hitTest:sliderPoint withEvent:event] : nil;
}
- (void)layoutSubviews {
    [super layoutSubviews];
    CGFloat w = self.bounds.size.width, h = self.bounds.size.height;
    if (!self.sensitivity) {
        NSArray *rects = @[
            [NSValue valueWithCGRect:CGRectMake(w*0.03,h*0.57,w*0.26,h*0.35)],
            [NSValue valueWithCGRect:CGRectMake(w*0.86,h*0.45,w*0.14,h*0.18)],
            [NSValue valueWithCGRect:CGRectMake(w*0.86,h*0.63,w*0.14,h*0.18)],
            [NSValue valueWithCGRect:CGRectMake(w*0.86,h*0.81,w*0.14,h*0.18)],
            [NSValue valueWithCGRect:CGRectMake(w*0.70,h*0.84,w*0.14,h*0.15)],
            [NSValue valueWithCGRect:CGRectMake(w*0.75,0,w*0.15,h*0.15)]];
        for (NSUInteger i=0; i<rects.count; i++) { self.subviews[i].frame = [rects[i] CGRectValue]; }
        return;
    }
    self.move.frame = CGRectMake(w*0.03, h*0.57, w*0.26, h*0.35);
    self.up.frame = CGRectMake(w*0.88, h*0.5, w*0.12, h*0.25);
    self.down.frame = CGRectMake(w*0.88, h*0.75, w*0.12, h*0.25);
    self.sensitivityLabel.frame = CGRectMake(w*0.48, h*0.03, w*0.23, 24);
    self.sensitivity.frame = CGRectMake(w*0.72, h*0.03, w*0.23, 28);
}
@end

void horizon_ios_status(const char *key) {
    NSCAssert([NSThread isMainThread], @"UI must be on the main thread");
    horizonStatus = [NSString stringWithUTF8String:key];
    UIWindow *window = horizonWindow;
    for (UIScene *scene in UIApplication.sharedApplication.connectedScenes) {
        if ([scene isKindOfClass:UIWindowScene.class]) {
            for (UIWindow *candidate in ((UIWindowScene *)scene).windows) {
                if (candidate.isKeyWindow) { window = candidate; break; }
            }
        }
    }
    // winit supports the legacy UIApplication window lifecycle too.
    if (!window) {
#pragma clang diagnostic push
#pragma clang diagnostic ignored "-Wdeprecated-declarations"
        for (UIWindow *candidate in UIApplication.sharedApplication.windows) {
            if (candidate.isKeyWindow) { window = candidate; break; }
        }
#pragma clang diagnostic pop
    }
    if (!window) { NSLog(@"Horizon: status window not yet available"); return; }
    static UILabel *label;
    if (!label) {
        label = [UILabel new];
        label.translatesAutoresizingMaskIntoConstraints = NO;
        label.numberOfLines = 0;
        label.font = [UIFont preferredFontForTextStyle:UIFontTextStyleCaption1];
        label.textColor = UIColor.whiteColor;
        label.backgroundColor = [UIColor colorWithWhite:0 alpha:0.65];
        label.userInteractionEnabled = NO;
        label.accessibilityIdentifier = @"horizon.diagnostic.status";
    }
    if (label.superview != window) {
        [label removeFromSuperview];
        [window addSubview:label];
        [NSLayoutConstraint activateConstraints:@[
            [label.leadingAnchor constraintEqualToAnchor:window.safeAreaLayoutGuide.leadingAnchor constant:12],
            [label.topAnchor constraintEqualToAnchor:window.safeAreaLayoutGuide.topAnchor constant:8],
            [label.trailingAnchor constraintLessThanOrEqualToAnchor:window.safeAreaLayoutGuide.trailingAnchor constant:-12]
        ]];
    }
    static HorizonFlightControls *controls;
    static BOOL controlsGameplay;
    BOOL gameplay = [horizonStatus hasPrefix:@"gameplay."];
    if (!controls || gameplay != controlsGameplay) {
        [controls removeFromSuperview];
        controls = [[HorizonFlightControls alloc] initWithFrame:window.bounds];
        controlsGameplay = gameplay;
    }
    controls.frame = window.bounds;
    controls.autoresizingMask = UIViewAutoresizingFlexibleWidth | UIViewAutoresizingFlexibleHeight;
    if (controls.superview != window) { [window addSubview:controls]; }
    controls.hidden = strcmp(key, "terrain.ready") != 0 && strcmp(key, "gameplay.ready") != 0;
    label.hidden = gameplay;
    label.text = NSLocalizedString([NSString stringWithUTF8String:key], nil);
}

// Native input mirrors the focused Horizon field; Rust remains authoritative.
extern void horizon_ios_navigation(uint32_t key);
extern void horizon_ios_submit(const char *identity);
extern void horizon_ios_edit(const char *identity, const char *value, NSUInteger cursor, NSUInteger anchor);
@interface HorizonTextView : UITextView
@end
@implementation HorizonTextView
- (NSArray<UIKeyCommand *> *)keyCommands {
    NSMutableArray<UIKeyCommand *> *commands = [[super keyCommands] mutableCopy] ?: [NSMutableArray new];
    UIKeyCommand *escape = [UIKeyCommand keyCommandWithInput:UIKeyInputEscape modifierFlags:0 action:@selector(horizonEscape:)];
    escape.wantsPriorityOverSystemBehavior = YES;
    [commands insertObject:escape atIndex:0];
    for (NSString *input in @[@"\b", @"\x7f"]) {
        UIKeyCommand *backspace = [UIKeyCommand keyCommandWithInput:input modifierFlags:0 action:@selector(horizonDelete:)];
        backspace.wantsPriorityOverSystemBehavior = YES;
        [commands insertObject:backspace atIndex:0];
    }
    return commands;
}
- (void)horizonEscape:(UIKeyCommand *)command { horizon_ios_navigation(0); }
- (void)horizonDelete:(UIKeyCommand *)command { [self deleteBackward]; }
- (void)insertText:(NSString *)text {
    // UIKeyInput clients can deliver either conventional Backspace control.
    if ([text isEqualToString:@"\b"] || [text isEqualToString:@"\x7f"]) { [self deleteBackward]; }
    else { [super insertText:text]; }
}
@end
// A regular UIView owns keyboard layout. UIWindow's guide does not track the
// keyboard in winit's legacy-window/scene attachment path.
@interface HorizonInputOverlay : UIView
@end
@implementation HorizonInputOverlay
- (UIView *)hitTest:(CGPoint)point withEvent:(UIEvent *)event {
    UIView *hit = [super hitTest:point withEvent:event];
    return hit == self ? nil : hit;
}
@end
@interface HorizonInput : NSObject <UITextViewDelegate>
@property(nonatomic, strong) HorizonInputOverlay *overlay;
@property(nonatomic, strong) UIButton *back;
@property(nonatomic, strong) UIButton *chat;
@property(nonatomic, strong) HorizonTextView *editor;
@property(nonatomic, copy) NSString *identity;
@property(nonatomic, copy) NSString *dismissed;
@property(nonatomic) BOOL applying;
@property(nonatomic) BOOL multiline;
@end
static HorizonInput *horizonInput;
@implementation HorizonInput
- (UIButton *)button:(NSString *)key symbol:(NSString *)symbol action:(SEL)action {
    UIButton *button = [UIButton buttonWithType:UIButtonTypeSystem];
    button.translatesAutoresizingMaskIntoConstraints = NO;
    button.backgroundColor = [UIColor colorWithWhite:0 alpha:0.55];
    button.tintColor = UIColor.whiteColor;
    button.layer.cornerRadius = 12;
    [button setImage:[UIImage systemImageNamed:symbol] forState:UIControlStateNormal];
    button.accessibilityLabel = NSLocalizedString(key, nil);
    button.accessibilityIdentifier = key;
    [button addTarget:self action:action forControlEvents:UIControlEventTouchUpInside];
    return button;
}
- (void)attach:(UIWindow *)window {
    if (self.overlay.superview == window) { return; }
    [self.overlay removeFromSuperview];
    self.overlay = [[HorizonInputOverlay alloc] initWithFrame:window.bounds];
    self.overlay.autoresizingMask = UIViewAutoresizingFlexibleWidth | UIViewAutoresizingFlexibleHeight;
    [window addSubview:self.overlay];
    self.back = [self button:@"navigation.back" symbol:@"arrow.uturn.backward" action:@selector(backPressed:)];
    self.chat = [self button:@"navigation.chat" symbol:@"text.bubble" action:@selector(chatPressed:)];
    self.editor = [HorizonTextView new];
    self.editor.translatesAutoresizingMaskIntoConstraints = NO;
    self.editor.delegate = self;
    self.editor.font = [UIFont preferredFontForTextStyle:UIFontTextStyleBody];
    self.editor.backgroundColor = UIColor.secondarySystemBackgroundColor;
    self.editor.textColor = UIColor.labelColor;
    self.editor.layer.cornerRadius = 8;
    self.editor.accessibilityLabel = NSLocalizedString(@"keyboard.focusedField", nil);
    self.editor.accessibilityIdentifier = @"horizon.nativeText";
    // ASCII-capable changes the layout, never the accepted character repertoire:
    // colon, brackets, DNS, IPv6, Unicode and paste all reach Horizon verbatim.
    self.editor.keyboardType = UIKeyboardTypeDefault;
    self.editor.autocapitalizationType = UITextAutocapitalizationTypeNone;
    self.editor.autocorrectionType = UITextAutocorrectionTypeNo;
    self.editor.smartQuotesType = UITextSmartQuotesTypeNo;
    self.editor.smartDashesType = UITextSmartDashesTypeNo;
    self.editor.smartInsertDeleteType = UITextSmartInsertDeleteTypeNo;
    UIToolbar *toolbar = [UIToolbar new]; [toolbar sizeToFit];
    toolbar.items = @[
        [[UIBarButtonItem alloc] initWithTitle:NSLocalizedString(@"keyboard.back", nil) style:UIBarButtonItemStylePlain target:self action:@selector(backPressed:)],
        [[UIBarButtonItem alloc] initWithBarButtonSystemItem:UIBarButtonSystemItemFlexibleSpace target:nil action:nil],
        [[UIBarButtonItem alloc] initWithBarButtonSystemItem:UIBarButtonSystemItemDone target:self action:@selector(donePressed:)]];
    self.editor.inputAccessoryView = toolbar;
    [self.overlay addSubview:self.back]; [self.overlay addSubview:self.chat]; [self.overlay addSubview:self.editor];
    self.overlay.keyboardLayoutGuide.followsUndockedKeyboard = YES;
    UILayoutGuide *safe = self.overlay.safeAreaLayoutGuide;
    [NSLayoutConstraint activateConstraints:@[
        [self.back.leadingAnchor constraintEqualToAnchor:safe.leadingAnchor constant:8],
        [self.back.topAnchor constraintEqualToAnchor:safe.topAnchor constant:4],
        [self.back.widthAnchor constraintEqualToConstant:44], [self.back.heightAnchor constraintEqualToConstant:44],
        [self.chat.leadingAnchor constraintEqualToAnchor:self.back.trailingAnchor constant:8],
        [self.chat.topAnchor constraintEqualToAnchor:self.back.topAnchor],
        [self.chat.widthAnchor constraintEqualToConstant:44], [self.chat.heightAnchor constraintEqualToConstant:44],
        [self.editor.leadingAnchor constraintEqualToAnchor:safe.leadingAnchor constant:8],
        [self.editor.trailingAnchor constraintEqualToAnchor:safe.trailingAnchor constant:-8],
        [self.editor.bottomAnchor constraintEqualToAnchor:self.overlay.keyboardLayoutGuide.topAnchor constant:-4],
        [self.editor.heightAnchor constraintEqualToConstant:60]
    ]];
    self.editor.hidden = YES;
    [self.overlay layoutIfNeeded];
}
- (void)commit {
    if (self.applying || !self.identity || self.editor.markedTextRange) { return; }
    NSRange range = self.editor.selectedRange;
    horizon_ios_edit(self.identity.UTF8String, self.editor.text.UTF8String, range.location + range.length, range.location);
}
- (void)finishComposition {
    [self.editor unmarkText]; [self commit];
}
- (void)backPressed:(id)sender {
    [self finishComposition];
    self.dismissed = self.identity;
    [self.editor resignFirstResponder];
    horizon_ios_navigation(0);
}
- (void)chatPressed:(id)sender { horizon_ios_navigation(2); }
- (void)donePressed:(id)sender {
    [self finishComposition]; self.dismissed = self.identity;
    [self.editor resignFirstResponder];
    if (!self.multiline) { horizon_ios_submit(self.identity.UTF8String); }
}
- (void)textViewDidChange:(UITextView *)textView { [self commit]; }
- (void)textViewDidChangeSelection:(UITextView *)textView { [self commit]; }
- (BOOL)textView:(UITextView *)textView shouldChangeTextInRange:(NSRange)range replacementText:(NSString *)text {
    if (!self.multiline && [text isEqualToString:@"\n"]) { [self donePressed:nil]; return NO; }
    return YES;
}
@end
uint32_t horizon_ios_text_active(void) { return horizonInput.editor.isFirstResponder ? 1 : 0; }
void horizon_ios_keyboard_reopen(void) {
    // Finish the old field's composition before a game touch can move focus.
    [horizonInput finishComposition];
    horizonInput.dismissed = nil;
}
void horizon_ios_input_state(const char *json) {
    NSCAssert(NSThread.isMainThread, @"Input synchronization requires the main thread");
    if (!horizonWindow) { return; }
    if (!horizonInput) { horizonInput = [HorizonInput new]; }
    [horizonInput attach:horizonWindow];
    NSData *data = [[NSString stringWithUTF8String:json] dataUsingEncoding:NSUTF8StringEncoding];
    id state = [NSJSONSerialization JSONObjectWithData:data options:NSJSONReadingFragmentsAllowed error:nil];
    BOOL ready = [state isKindOfClass:NSDictionary.class] && [state[@"ready"] boolValue];
    horizonInput.back.hidden = !ready;
    horizonInput.chat.hidden = !ready || ![state[@"playing"] boolValue];
    NSDictionary *field = ready && [state[@"field"] isKindOfClass:NSDictionary.class] ? state[@"field"] : nil;
    if (!field) {
        [horizonInput.editor resignFirstResponder]; horizonInput.editor.hidden = YES;
        horizonInput.identity = nil; horizonInput.dismissed = nil;
        return;
    }
    BOOL changed = ![horizonInput.identity isEqualToString:field[@"id"]];
    if (changed) {
        // Composition belongs to its original field; never commit it into a new one.
        horizonInput.applying = YES; [horizonInput.editor unmarkText]; horizonInput.applying = NO;
        horizonInput.identity = field[@"id"]; horizonInput.dismissed = nil;
    }
    horizonInput.multiline = [field[@"multiline"] boolValue];
    UIKeyboardType keyboardType = [field[@"address"] boolValue] ? UIKeyboardTypeURL : UIKeyboardTypeDefault;
    if (horizonInput.editor.keyboardType != keyboardType) {
        horizonInput.editor.keyboardType = keyboardType;
        [horizonInput.editor reloadInputViews];
    }
    horizonInput.editor.returnKeyType = horizonInput.multiline ? UIReturnKeyDefault : UIReturnKeyDone;
    if (!horizonInput.editor.markedTextRange) {
        horizonInput.applying = YES;
        if (![horizonInput.editor.text isEqualToString:field[@"value"]]) { horizonInput.editor.text = field[@"value"]; }
        NSUInteger a = MIN([field[@"cursor"] unsignedIntegerValue], horizonInput.editor.text.length);
        NSUInteger b = MIN([field[@"anchor"] unsignedIntegerValue], horizonInput.editor.text.length);
        NSRange selection = NSMakeRange(MIN(a,b), MAX(a,b)-MIN(a,b));
        if (!NSEqualRanges(horizonInput.editor.selectedRange, selection)) { horizonInput.editor.selectedRange = selection; }
        horizonInput.applying = NO;
    }
    BOOL dismissed = [horizonInput.dismissed isEqualToString:horizonInput.identity];
    horizonInput.editor.hidden = dismissed;
    if (!dismissed && horizonWindow.windowScene.activationState == UISceneActivationStateForegroundActive) {
        [horizonInput.editor becomeFirstResponder];
    }
}

int main(int argc, char **argv) {
    @autoreleasepool {
        NSFileManager *files = NSFileManager.defaultManager;
        NSURL *support = [[files URLsForDirectory:NSApplicationSupportDirectory inDomains:NSUserDomainMask].firstObject URLByAppendingPathComponent:@"HorizonIOSDiagnostic" isDirectory:YES];
        NSURL *cache = [[files URLsForDirectory:NSCachesDirectory inDomains:NSUserDomainMask].firstObject URLByAppendingPathComponent:@"HorizonIOSDiagnostic" isDirectory:YES];
        // Exposed by UIFileSharingEnabled; a player can copy HorizonResources here.
        NSURL *documents = [files URLsForDirectory:NSDocumentDirectory inDomains:NSUserDomainMask].firstObject;
        horizon_ios_run(support.fileSystemRepresentation, cache.fileSystemRepresentation,
                        NSBundle.mainBundle.bundlePath.fileSystemRepresentation,
                        documents.fileSystemRepresentation);
    }
    return 0;
}
