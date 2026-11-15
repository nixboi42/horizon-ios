// winit calls UIApplicationMain and owns the app delegate/view controller.
// Do not add a second UIKit or MTKView render loop here.
#import <UIKit/UIKit.h>
#import <QuartzCore/QuartzCore.h>
#import <mach/mach.h>
// CPAL owns RemoteIO. This bridge owns only Apple's app-wide audio session.
#import <AVFoundation/AVFoundation.h>
#import <stdatomic.h>
static atomic_uint_fast64_t horizonAudioState;
static NSUInteger horizonAudioOwners;
static BOOL horizonAudioBackground;
static BOOL horizonAudioInterrupted;
static NSArray *horizonAudioObservers;
static void horizonAudioPublish(BOOL active) {
    uint64_t old = atomic_load(&horizonAudioState);
    atomic_store(&horizonAudioState, ((old + 2) & ~1ULL) | (active ? 1 : 0));
}
static void horizonAudioActivate(void) {
    if (!horizonAudioOwners || horizonAudioBackground || horizonAudioInterrupted) { return; }
    AVAudioSession *session = AVAudioSession.sharedInstance;
    NSError *error = nil;
    // Playback follows the system route (including Bluetooth A2DP/headphones),
    // requests no microphone, and stops when the app resigns active.
    BOOL ok = [session setCategory:AVAudioSessionCategoryPlayback mode:AVAudioSessionModeDefault options:0 error:&error];
    if (ok) { [session setPreferredSampleRate:48000 error:nil]; }
    if (ok) { [session setPreferredIOBufferDuration:0.010666667 error:nil]; }
    if (ok) { ok = [session setActive:YES error:&error]; }
    horizonAudioPublish(ok);
    if (!ok) { NSLog(@"Horizon audio session: %@", error.localizedDescription); }
}
void horizon_ios_audio_begin(void) {
    @synchronized (AVAudioSession.sharedInstance) {
        if (horizonAudioOwners++ != 0) { return; }
        NSNotificationCenter *nc = NSNotificationCenter.defaultCenter;
        NSMutableArray *observers = [NSMutableArray new];
        void (^observe)(NSNotificationName, void (^)(NSNotification *)) = ^(NSNotificationName name, void (^action)(NSNotification *)) {
            [observers addObject:[nc addObserverForName:name object:nil queue:NSOperationQueue.mainQueue usingBlock:^(NSNotification *note) {
                @synchronized (AVAudioSession.sharedInstance) { if (horizonAudioOwners) { action(note); } }
            }]];
        };
        observe(AVAudioSessionInterruptionNotification, ^(NSNotification *note) {
            BOOL began = [note.userInfo[AVAudioSessionInterruptionTypeKey] unsignedIntegerValue] == AVAudioSessionInterruptionTypeBegan;
            BOOL resume = !began && ([note.userInfo[AVAudioSessionInterruptionOptionKey] unsignedIntegerValue] & AVAudioSessionInterruptionOptionShouldResume);
            horizonAudioInterrupted = !resume;
            if (began) { horizonAudioPublish(NO); }
            else if (resume) { horizonAudioActivate(); }
        });
        observe(UIApplicationWillResignActiveNotification, ^(NSNotification *note) {
            horizonAudioBackground = YES;
            horizonAudioPublish(NO);
            [AVAudioSession.sharedInstance setActive:NO withOptions:AVAudioSessionSetActiveOptionNotifyOthersOnDeactivation error:nil];
        });
        observe(UIApplicationDidBecomeActiveNotification, ^(NSNotification *note) {
            horizonAudioBackground = NO; horizonAudioInterrupted = NO; horizonAudioActivate();
        });
        observe(AVAudioSessionRouteChangeNotification, ^(NSNotification *note) { horizonAudioActivate(); });
        observe(AVAudioSessionMediaServicesWereLostNotification, ^(NSNotification *note) { horizonAudioPublish(NO); });
        observe(AVAudioSessionMediaServicesWereResetNotification, ^(NSNotification *note) { horizonAudioActivate(); });
        horizonAudioObservers = observers;
        horizonAudioActivate();
    }
}
void horizon_ios_audio_suspend(void) {
    @synchronized (AVAudioSession.sharedInstance) {
        horizonAudioBackground = YES;
        horizonAudioPublish(NO);
        [AVAudioSession.sharedInstance setActive:NO withOptions:AVAudioSessionSetActiveOptionNotifyOthersOnDeactivation error:nil];
    }
}
uint64_t horizon_ios_audio_state(void) { return atomic_load(&horizonAudioState); }
void horizon_ios_audio_end(void) {
    @synchronized (AVAudioSession.sharedInstance) {
        if (!horizonAudioOwners || --horizonAudioOwners) { return; }
        for (id observer in horizonAudioObservers) { [NSNotificationCenter.defaultCenter removeObserver:observer]; }
        horizonAudioObservers = nil;
        horizonAudioPublish(NO);
        [AVAudioSession.sharedInstance setActive:NO withOptions:AVAudioSessionSetActiveOptionNotifyOthersOnDeactivation error:nil];
        horizonAudioInterrupted = NO;
    }
}


// 2 GB hardware reports usable RAM, which can be slightly below its nominal size.
int32_t horizon_ios_constrained_memory(void) {
    return NSProcessInfo.processInfo.physicalMemory <= (2ULL << 30)
        || ![NSProcessInfo.processInfo isOperatingSystemAtLeastVersion:(NSOperatingSystemVersion){16, 0, 0}];
}
void horizon_ios_footprint(uint64_t *current, uint64_t *peak) {
    task_vm_info_data_t info = {0};
    mach_msg_type_number_t count = TASK_VM_INFO_COUNT;
    if (task_info(mach_task_self(), TASK_VM_INFO, (task_info_t)&info, &count) != KERN_SUCCESS) { return; }
    if (count >= TASK_VM_INFO_REV1_COUNT) { *current = info.phys_footprint; }
    if (count >= TASK_VM_INFO_REV3_COUNT && info.ledger_phys_footprint_peak > 0) {
        *peak = (uint64_t)info.ledger_phys_footprint_peak;
    }
}

// iOS 27 requires scene adoption. winit 0.30 still creates the UIWindow and
// view controller; this delegate only supplies its public UIWindowScene.
static UIWindowScene *horizonScene;
static UIWindow *horizonWindow;
static __weak UIView *horizonGameView;
static NSString *horizonStatus;
void horizon_ios_status(const char *key);
void horizon_ios_input_state(const char *json);
void horizon_ios_launcher_attach(void *window);
void horizon_ios_launcher_status(const char *key);
static void attachWindow(void) {
    if (!horizonWindow) { return; }
    if (horizonScene && horizonWindow.windowScene != horizonScene) {
        horizonWindow.windowScene = horizonScene;
        horizonWindow.frame = horizonScene.coordinateSpace.bounds;
    }
    [horizonWindow makeKeyAndVisible];
    [horizonWindow layoutIfNeeded];
    NSLog(@"Horizon scene attached: %@", NSStringFromCGRect(horizonWindow.bounds));
    horizon_ios_launcher_attach((__bridge void *)horizonWindow);
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
    horizonGameView = view;
    horizonWindow = view.window;
    attachWindow();
}

void horizon_ios_touch_insets(void *rawView, double *out) {
    UIEdgeInsets i = ((__bridge UIView *)rawView).safeAreaInsets;
    out[0] = i.top; out[1] = i.left; out[2] = i.bottom; out[3] = i.right;
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
        NSArray *keys = @[@"flight.move", @"flight.up", @"flight.down", @"flight.sensitivity"];
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
    CGPoint sliderPoint = [self.sensitivity convertPoint:point fromView:self];
    return [self.sensitivity pointInside:sliderPoint withEvent:event] ? [self.sensitivity hitTest:sliderPoint withEvent:event] : nil;
}
- (void)layoutSubviews {
    [super layoutSubviews];
    CGFloat w = self.bounds.size.width, h = self.bounds.size.height;
    self.move.frame = CGRectMake(w*0.03, h*0.57, w*0.26, h*0.35);
    self.up.frame = CGRectMake(w*0.88, h*0.5, w*0.12, h*0.25);
    self.down.frame = CGRectMake(w*0.88, h*0.75, w*0.12, h*0.25);
    self.sensitivityLabel.frame = CGRectMake(w*0.48, h*0.03, w*0.23, 24);
    self.sensitivity.frame = CGRectMake(w*0.72, h*0.03, w*0.23, 28);
}
@end

void horizon_ios_status(const char *key) {
    NSCAssert([NSThread isMainThread], @"UI must be on the main thread");
    BOOL wasGameplay = [horizonStatus hasPrefix:@"gameplay."];
    horizonStatus = [NSString stringWithUTF8String:key];
    if (wasGameplay && [horizonStatus hasPrefix:@"launcher."]) {
        // Rust has dropped the session. wgpu's Metal sublayers are still
        // retained by the persistent winit view and can cover SwiftUI with
        // the last game frame. The next Play creates its own surface.
        for (CALayer *layer in [horizonGameView.layer.sublayers copy]) {
            if ([layer isKindOfClass:CAMetalLayer.class]) {
                [layer removeFromSuperlayer];
            }
        }
    }
    horizon_ios_launcher_status(key);
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
    if (strcmp(key, "terrain.ready") == 0) {
        if (!controls) {
            controls = [[HorizonFlightControls alloc] initWithFrame:window.bounds];
            controls.autoresizingMask = UIViewAutoresizingFlexibleWidth | UIViewAutoresizingFlexibleHeight;
        }
        controls.frame = window.bounds;
        if (controls.superview != window) { [window addSubview:controls]; }
    } else {
        [controls removeFromSuperview];
    }
    BOOL gameplay = [horizonStatus hasPrefix:@"gameplay."];
    label.hidden = gameplay || [horizonStatus hasPrefix:@"launcher."];
    label.text = NSLocalizedString([NSString stringWithUTF8String:key], nil);
}

// Native input mirrors the focused Horizon field; Rust remains authoritative.
extern void horizon_ios_navigation(uint32_t key);
extern void horizon_ios_chat(void);
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
#import "TouchControls.h"
@interface HorizonInput : NSObject <UITextViewDelegate>
@property(nonatomic, strong) HorizonInputOverlay *overlay;
@property(nonatomic, strong) UIButton *back;
@property(nonatomic, strong) HorizonTouchUI *touchUI;
@property(nonatomic) BOOL ready;
- (void)updateNavigation;
@property(nonatomic, strong) HorizonTextView *editor;
@property(nonatomic, copy) NSString *identity;
@property(nonatomic, strong) UIView *silentInputView;
@property(nonatomic, strong) UIToolbar *keyboardToolbar;
@property(nonatomic, strong) NSLayoutConstraint *editorBottom;
@property(nonatomic, copy) NSString *pendingKeyboard;
@property(nonatomic) BOOL keyboardRequested;
@property(nonatomic) NSUInteger keyboardRequest;
@property(nonatomic) NSUInteger keyboardHidingRequest;
@property(nonatomic) BOOL applying;
@property(nonatomic) BOOL multiline;
- (void)finishComposition;
- (void)hideKeyboard;
@end
static HorizonInput *horizonInput;
@implementation HorizonInput
- (instancetype)init {
    if ((self = [super init])) {
        if (![NSProcessInfo.processInfo isOperatingSystemAtLeastVersion:(NSOperatingSystemVersion){16, 0, 0}]) {
            [NSNotificationCenter.defaultCenter addObserver:self selector:@selector(keyboardFrame:)
                                                       name:UIKeyboardWillChangeFrameNotification object:nil];
        }
        [NSNotificationCenter.defaultCenter addObserver:self selector:@selector(keyboardWillHide:)
                                                   name:UIKeyboardWillHideNotification object:nil];
        [NSNotificationCenter.defaultCenter addObserver:self selector:@selector(keyboardDidHide:)
                                                   name:UIKeyboardDidHideNotification object:nil];
    }
    return self;
}
- (void)dealloc { [NSNotificationCenter.defaultCenter removeObserver:self]; }
- (void)configureKeyboard:(BOOL)show {
    self.keyboardRequested = show;
    // Keep UIKit's full hardware editing and composition even when the
    // Minecraft widget is the only visible editor.
    self.editor.inputView = show ? nil : self.silentInputView;
    self.editor.inputAccessoryView = show ? self.keyboardToolbar : nil;
    self.editor.alpha = show ? 1 : 0;
    self.editor.accessibilityElementsHidden = !show;
}
- (void)keyboardFrame:(NSNotification *)notification {
    UIWindow *window = self.overlay.window;
    if (!window.isKeyWindow || !self.keyboardRequested) { return; }
    CGRect screenFrame = [notification.userInfo[UIKeyboardFrameEndUserInfoKey] CGRectValue];
    CGRect windowFrame = [window convertRect:screenFrame fromCoordinateSpace:window.screen.coordinateSpace];
    CGRect localFrame = [self.overlay convertRect:windowFrame fromView:window];
    CGRect overlap = CGRectIntersection(self.overlay.bounds, localFrame);
    CGFloat covered = CGRectIsNull(overlap) ? 0 : CGRectGetHeight(overlap);
    self.editorBottom.constant = -4 - MAX(0, covered - self.overlay.safeAreaInsets.bottom);
    [UIView animateWithDuration:[notification.userInfo[UIKeyboardAnimationDurationUserInfoKey] doubleValue]
                     animations:^{ [self.overlay layoutIfNeeded]; }];
}
- (void)keyboardWillHide:(NSNotification *)notification {
    // A completed hide animation may belong to an older request.
    self.keyboardHidingRequest = self.keyboardRequested ? self.keyboardRequest : 0;
}
- (void)keyboardDidHide:(NSNotification *)notification {
    if (self.keyboardRequested && self.keyboardHidingRequest == self.keyboardRequest) {
        [self finishComposition];
        [self hideKeyboard];
    }
    self.keyboardHidingRequest = 0;
}
- (void)clearEditor {
    // A screen change can invalidate the field before UIKit ends composition.
    self.identity = nil;
    self.pendingKeyboard = nil;
    self.applying = YES;
    [self configureKeyboard:NO];
    [self.editor unmarkText];
    [self.editor resignFirstResponder];
    self.editor.hidden = YES;
    self.applying = NO;
}
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
    [self clearEditor];
    [self.overlay removeFromSuperview];
    self.overlay = [[HorizonInputOverlay alloc] initWithFrame:window.bounds];
    self.overlay.autoresizingMask = UIViewAutoresizingFlexibleWidth | UIViewAutoresizingFlexibleHeight;
    [window addSubview:self.overlay];
    self.back = [self button:@"navigation.back" symbol:@"arrow.uturn.backward" action:@selector(backPressed:)];
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
    self.keyboardToolbar = toolbar;
    self.silentInputView = [[UIView alloc] initWithFrame:CGRectZero];
    [self configureKeyboard:NO];
    [self.overlay addSubview:self.back]; [self.overlay addSubview:self.editor];
    UILayoutGuide *safe = self.overlay.safeAreaLayoutGuide;
    if (@available(iOS 16, *)) {
        self.overlay.keyboardLayoutGuide.followsUndockedKeyboard = YES;
        self.editorBottom = [self.editor.bottomAnchor constraintEqualToAnchor:self.overlay.keyboardLayoutGuide.topAnchor constant:-4];
    } else {
        self.editorBottom = [self.editor.bottomAnchor constraintEqualToAnchor:safe.bottomAnchor constant:-4];
    }
    [NSLayoutConstraint activateConstraints:@[
        [self.back.leadingAnchor constraintEqualToAnchor:safe.leadingAnchor constant:8],
        [self.back.topAnchor constraintEqualToAnchor:safe.topAnchor constant:4],
        [self.back.widthAnchor constraintEqualToConstant:44], [self.back.heightAnchor constraintEqualToConstant:44],
        [self.editor.leadingAnchor constraintEqualToAnchor:safe.leadingAnchor constant:8],
        [self.editor.trailingAnchor constraintEqualToAnchor:safe.trailingAnchor constant:-8],
        self.editorBottom,
        [self.editor.heightAnchor constraintEqualToConstant:60]
    ]];
    self.editor.hidden = YES;
    self.touchUI = [HorizonTouchUI new];
    [self.touchUI attach:self.overlay];
    [self.overlay layoutIfNeeded];
}
- (void)updateNavigation {
    uint32_t mode = self.touchUI.mode;
    self.back.hidden = !self.ready || mode == 1 || mode == 3 || mode == 4;
}
- (void)commit {
    if (self.applying || !self.identity || self.editor.markedTextRange) { return; }
    NSRange range = self.editor.selectedRange;
    horizon_ios_edit(self.identity.UTF8String, self.editor.text.UTF8String, range.location + range.length, range.location);
}
- (void)finishComposition {
    if (self.editor.markedTextRange) {
        [self.editor unmarkText];
        [self commit];
    }
}
- (void)backPressed:(id)sender {
    [self finishComposition];
    [self hideKeyboard];
    horizon_ios_navigation(0);
}
- (void)hideKeyboard {
    if (!self.keyboardRequested) { return; }
    [self configureKeyboard:NO];
    self.pendingKeyboard = nil;
    if (@available(iOS 16, *)) {
        [self.editor reloadInputViews];
    } else {
        self.editorBottom.constant = -4;
        [self.editor resignFirstResponder];
    }
}
- (void)donePressed:(id)sender {
    [self finishComposition];
    [self hideKeyboard];
    if (!self.multiline) { horizon_ios_submit(self.identity.UTF8String); }
}
- (void)textViewDidChange:(UITextView *)textView { [self commit]; }
- (void)textViewDidChangeSelection:(UITextView *)textView { [self commit]; }
- (BOOL)textView:(UITextView *)textView shouldChangeTextInRange:(NSRange)range replacementText:(NSString *)text {
    if (!self.multiline && [text isEqualToString:@"\n"]) { [self donePressed:nil]; return NO; }
    return YES;
}
@end
void horizon_ios_touch_frame(const HorizonTouchFrame *frame) {
    if (!horizonInput || !horizonWindow) { return; }
    [horizonInput.touchUI update:frame];
    [horizonInput updateNavigation];
}
void horizon_ios_touch_save_failed(void) {
    UIAlertController *alert = [UIAlertController alertControllerWithTitle:TouchText(@"touch.saveFailed")
        message:nil preferredStyle:UIAlertControllerStyleAlert];
    [alert addAction:[UIAlertAction actionWithTitle:TouchText(@"touch.ok") style:UIAlertActionStyleDefault handler:nil]];
    [horizonWindow.rootViewController presentViewController:alert animated:YES completion:nil];
}
uint32_t horizon_ios_text_active(void) { return horizonInput.editor.isFirstResponder ? 1 : 0; }
void horizon_ios_finish_composition(void) { [horizonInput finishComposition]; }
void horizon_ios_input_state(const char *json) {
    NSCAssert(NSThread.isMainThread, @"Input synchronization requires the main thread");
    if (!horizonWindow) { return; }
    if (!horizonInput) { horizonInput = [HorizonInput new]; }
    [horizonInput attach:horizonWindow];
    NSData *data = [[NSString stringWithUTF8String:json] dataUsingEncoding:NSUTF8StringEncoding];
    id state = [NSJSONSerialization JSONObjectWithData:data options:NSJSONReadingFragmentsAllowed error:nil];
    BOOL ready = [state isKindOfClass:NSDictionary.class] && [state[@"ready"] boolValue];
    horizonInput.ready = ready;
    [horizonInput updateNavigation];
    NSDictionary *field = ready && [state[@"field"] isKindOfClass:NSDictionary.class] ? state[@"field"] : nil;
    if (!field) {
        [horizonInput clearEditor];
        return;
    }
    NSString *identity = field[@"id"];
    if ([state[@"show_keyboard"] boolValue]) { horizonInput.pendingKeyboard = identity; }
    BOOL requested = [horizonInput.pendingKeyboard isEqualToString:identity];
    if (!requested) { horizonInput.pendingKeyboard = nil; }
    UIWindowScene *scene = horizonWindow.windowScene;
    BOOL active = scene ? scene.activationState == UISceneActivationStateForegroundActive
                        : UIApplication.sharedApplication.applicationState == UIApplicationStateActive;
    // An explicit tap may arrive before UIKit's key-window handoff. Keep the
    // request scoped to this field until the window is eligible for focus.
    if (!active || !horizonWindow.isKeyWindow || horizonInput.editor.window != horizonWindow) { return; }
    BOOL changed = ![horizonInput.identity isEqualToString:identity];
    BOOL show = requested || (!changed && horizonInput.keyboardRequested);
    BOOL reload = show != horizonInput.keyboardRequested;
    if (changed) {
        // Composition belongs to the old field.
        horizonInput.applying = YES;
        [horizonInput.editor unmarkText];
        horizonInput.applying = NO;
        horizonInput.identity = field[@"id"];
    }
    if (requested) { ++horizonInput.keyboardRequest; }
    if (reload && horizonInput.editor.isFirstResponder
        && ![NSProcessInfo.processInfo isOperatingSystemAtLeastVersion:(NSOperatingSystemVersion){16, 0, 0}]) {
        // iOS 15 can keep the old custom inputView after reloadInputViews.
        // Resign while it is still the silent view, then claim the real keyboard.
        [horizonInput.editor resignFirstResponder];
    }
    if (reload) { [horizonInput configureKeyboard:show]; }
    horizonInput.multiline = [field[@"multiline"] boolValue];
    UIKeyboardType keyboardType = [field[@"address"] boolValue] ? UIKeyboardTypeURL : UIKeyboardTypeDefault;
    UIReturnKeyType returnKey = horizonInput.multiline ? UIReturnKeyDefault : UIReturnKeyDone;
    reload |= show && (horizonInput.editor.keyboardType != keyboardType
                       || horizonInput.editor.returnKeyType != returnKey);
    horizonInput.editor.keyboardType = keyboardType;
    horizonInput.editor.returnKeyType = returnKey;
    if (!horizonInput.editor.markedTextRange) {
        horizonInput.applying = YES;
        if (![horizonInput.editor.text isEqualToString:field[@"value"]]) { horizonInput.editor.text = field[@"value"]; }
        NSUInteger a = MIN([field[@"cursor"] unsignedIntegerValue], horizonInput.editor.text.length);
        NSUInteger b = MIN([field[@"anchor"] unsignedIntegerValue], horizonInput.editor.text.length);
        NSRange selection = NSMakeRange(MIN(a,b), MAX(a,b)-MIN(a,b));
        if (!NSEqualRanges(horizonInput.editor.selectedRange, selection)) { horizonInput.editor.selectedRange = selection; }
        horizonInput.applying = NO;
    }
    horizonInput.editor.hidden = NO;
    if (!horizonInput.editor.isFirstResponder && (!show || requested)) {
        [horizonInput.editor becomeFirstResponder];
    } else if (reload && horizonInput.editor.isFirstResponder) {
        [horizonInput.editor reloadInputViews];
    }
    if (requested && horizonInput.editor.isFirstResponder) {
        horizonInput.pendingKeyboard = nil;
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
