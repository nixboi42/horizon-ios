// Included by main.m so both the development and distribution projects use it.
enum { TouchCount = 10 };
typedef struct {
    double x, y, side, opacity;
    uint32_t down;
} HorizonTouchButton;
typedef struct {
    HorizonTouchButton buttons[TouchCount];
    double knob[2];
    uint32_t scheme, mode;
    int32_t selected;
    uint32_t flying;
    double values[9], selection[2];
} HorizonTouchFrame;

extern void horizon_ios_touch_command(uint32_t command, double value);

static NSString *TouchText(NSString *key) { return NSLocalizedString(key, nil); }

@interface HorizonTouchTile : UIView
@property(nonatomic) NSInteger icon;
@property(nonatomic) BOOL pad, flying, pressed, selected;
@property(nonatomic) CGPoint knob;
@end
@implementation HorizonTouchTile
- (BOOL)accessibilityActivate {
    if (self.icon == 8 || self.icon == 9) {
        horizon_ios_touch_command(self.icon == 8 ? 30 : 31, 0);
        return YES;
    }
    return NO;
}
- (void)drawRect:(CGRect)rect {
    CGContextRef ctx = UIGraphicsGetCurrentContext();
    CGFloat d = self.bounds.size.width;
    if (d <= 0) { return; }
    CGContextScaleCTM(ctx, d / 100, d / 100);
    CGRect box = CGRectMake(1.5, 1.5, 97, 97);
    UIBezierPath *back = self.icon == 0 && !self.pad
        ? [UIBezierPath bezierPathWithOvalInRect:box]
        : [UIBezierPath bezierPathWithRoundedRect:box cornerRadius:16];
    [[UIColor colorWithWhite:self.pressed ? 0.32 : 0.055 alpha:0.78] setFill];
    [back fill];
    [(self.selected ? UIColor.systemYellowColor : [UIColor colorWithWhite:1 alpha:0.78]) setStroke];
    back.lineWidth = self.selected ? 4 : 2.5;
    [back stroke];
    [UIColor.whiteColor setStroke];
    [UIColor.whiteColor setFill];
    if (self.icon == 0 && !self.pad) {
        CGPoint k = self.knob;
        CGRect thumb = CGRectMake(33 + 29 * k.x, 33 + 29 * k.y, 34, 34);
        UIBezierPath *circle = [UIBezierPath bezierPathWithOvalInRect:thumb];
        [[UIColor colorWithWhite:1 alpha:0.48] setFill];
        [circle fill];
        circle.lineWidth = 2;
        [circle stroke];
        return;
    }
    CGContextSetLineWidth(ctx, 5);
    CGContextSetLineCap(ctx, kCGLineCapRound);
    CGContextSetLineJoin(ctx, kCGLineJoinRound);
    #define L(X,Y) CGContextAddLineToPoint(ctx,(X),(Y))
    #define M(X,Y) CGContextMoveToPoint(ctx,(X),(Y))
    #define CHEVRON(X,Y,DIR) M((X)-13,(Y)+9*(DIR)); L(X,Y); L((X)+13,(Y)+9*(DIR))
    switch (self.icon) {
        case 0:
            CHEVRON(50,13,1); CHEVRON(50,87,-1);
            M(24,37); L(13,50); L(24,63); M(76,37); L(87,50); L(76,63);
            CGContextStrokePath(ctx);
            CGContextFillEllipseInRect(ctx, CGRectMake(44 + 18*self.knob.x,44 + 18*self.knob.y,12,12));
            break;
        case 1:
            CHEVRON(50,32,1); M(50,32); L(50,68);
            if (self.flying) { CHEVRON(50,51,1); }
            CGContextStrokePath(ctx);
            break;
        case 2:
            CHEVRON(50,62,-1); M(50,30); L(50,62);
            if (self.flying) { CHEVRON(50,46,-1); }
            else { M(32,74); L(68,74); }
            CGContextStrokePath(ctx);
            break;
        case 3:
            M(25,33); L(42,50); L(25,67); M(46,33); L(63,50); L(46,67);
            M(67,33); L(84,50); L(67,67); CGContextStrokePath(ctx);
            break;
        case 4:
            M(27,73); L(60,40); M(38,51); L(50,63); M(48,48); L(69,27); L(75,25);
            L(74,37); L(53,58); CGContextStrokePath(ctx);
            break;
        case 5:
            M(50,23); L(73,35); L(73,62); L(50,76); L(27,62); L(27,35); L(50,23);
            M(27,35); L(50,49); L(73,35); M(50,49); L(50,76); CGContextStrokePath(ctx);
            break;
        case 6:
            for (int row=0;row<3;row++) for (int col=0;col<3;col++)
                CGContextFillRect(ctx, CGRectMake(28+17*col,28+17*row,10,10));
            break;
        case 7:
            M(21,50); CGContextAddQuadCurveToPoint(ctx,50,19,79,50);
            CGContextAddQuadCurveToPoint(ctx,50,81,21,50); CGContextStrokePath(ctx);
            CGContextFillEllipseInRect(ctx, CGRectMake(42,42,16,16)); break;
        case 8:
            M(26,29); L(74,29); L(74,62); L(49,62); L(35,73); L(35,62); L(26,62); L(26,29);
            CGContextStrokePath(ctx); break;
        case 9:
            M(40,31); L(40,69); M(60,31); L(60,69); CGContextStrokePath(ctx); break;
    }
    #undef CHEVRON
    #undef M
    #undef L
}
@end

@interface HorizonTouchUI : NSObject <UIPickerViewDataSource, UIPickerViewDelegate>
@property(nonatomic,strong) UIView *surface;
@property(nonatomic,strong) NSArray<HorizonTouchTile *> *tiles;
@property(nonatomic,strong) UIButton *launcher;
@property(nonatomic,strong) UIView *panel;
@property(nonatomic,strong) UIStackView *toolbar;
@property(nonatomic,strong) UISlider *buttonSize;
@property(nonatomic,strong) UISlider *buttonOpacity;
@property(nonatomic,strong) UIButton *reset;
@property(nonatomic) uint32_t mode;
@property(nonatomic) BOOL flying;
@end

@implementation HorizonTouchUI
- (UIButton *)button:(NSString *)key command:(NSInteger)command {
    UIButton *b = [UIButton buttonWithType:UIButtonTypeSystem];
    [b setTitle:TouchText(key) forState:UIControlStateNormal];
    b.accessibilityIdentifier = key;
    b.titleLabel.font = [UIFont systemFontOfSize:14 weight:UIFontWeightSemibold];
    b.titleLabel.adjustsFontSizeToFitWidth = YES;
    b.titleLabel.minimumScaleFactor = 0.6;
    b.tintColor = UIColor.whiteColor;
    b.backgroundColor = [UIColor colorWithWhite:0.07 alpha:0.94];
    b.layer.cornerRadius = 7;
    b.contentEdgeInsets = UIEdgeInsetsMake(0, 7, 0, 7);
    b.tag = command;
    [b addTarget:self action:@selector(command:) forControlEvents:UIControlEventTouchUpInside];
    return b;
}
- (void)command:(UIButton *)sender { horizon_ios_touch_command((uint32_t)sender.tag, 0); }
- (void)value:(UIControl *)sender {
    double v = 0;
    if ([sender isKindOfClass:UISlider.class]) { v = ((UISlider *)sender).value; }
    else if ([sender isKindOfClass:UISwitch.class]) { v = ((UISwitch *)sender).on; }
    else if ([sender isKindOfClass:UISegmentedControl.class]) { v = ((UISegmentedControl *)sender).selectedSegmentIndex; }
    horizon_ios_touch_command((uint32_t)sender.tag, v);
}
- (UISlider *)slider:(uint32_t)tag min:(float)min max:(float)max current:(float)value {
    UISlider *s = [UISlider new];
    s.minimumValue = min; s.maximumValue = max; s.value = value; s.tag = tag;
    [s addTarget:self action:@selector(value:) forControlEvents:UIControlEventValueChanged];
    return s;
}
- (UILabel *)label:(NSString *)key {
    UILabel *label = [UILabel new];
    label.text = TouchText(key);
    label.textColor = UIColor.whiteColor;
    label.font = [UIFont systemFontOfSize:15];
    label.numberOfLines = 0;
    return label;
}
- (UIStackView *)row:(NSString *)key control:(UIControl *)control {
    UILabel *label = [self label:key];
    control.accessibilityLabel = label.text;
    UIStackView *row = [[UIStackView alloc] initWithArrangedSubviews:@[label, control]];
    row.alignment = UIStackViewAlignmentCenter; row.spacing = 12;
    [control.widthAnchor constraintGreaterThanOrEqualToConstant:52].active = YES;
    [label setContentHuggingPriority:UILayoutPriorityDefaultHigh forAxis:UILayoutConstraintAxisHorizontal];
    [row.heightAnchor constraintGreaterThanOrEqualToConstant:40].active = YES;
    return row;
}
- (void)attach:(UIView *)surface {
    if (self.surface == surface) { return; }
    self.surface = surface;
    self.mode = UINT32_MAX;
    NSMutableArray *tiles = [NSMutableArray arrayWithCapacity:TouchCount];
    for (NSInteger i = 0; i < TouchCount; i++) {
        HorizonTouchTile *tile = [HorizonTouchTile new];
        tile.icon = i;
        tile.opaque = NO;
        tile.userInteractionEnabled = NO;
        tile.hidden = YES;
        tile.contentMode = UIViewContentModeRedraw;
        if (i == 8 || i == 9) {
            NSString *key = i == 8 ? @"navigation.chat" : @"navigation.back";
            tile.accessibilityIdentifier = key;
            tile.accessibilityLabel = TouchText(i == 8 ? key : @"touch.pause");
            tile.accessibilityTraits = UIAccessibilityTraitButton;
        }
        [surface insertSubview:tile atIndex:0];
        [tiles addObject:tile];
    }
    self.tiles = tiles;
    self.launcher = nil;
    self.panel = nil;
    self.toolbar = nil;
}
- (NSInteger)numberOfComponentsInPickerView:(UIPickerView *)picker { return 1; }
- (NSInteger)pickerView:(UIPickerView *)picker numberOfRowsInComponent:(NSInteger)component { return 3; }
- (NSAttributedString *)pickerView:(UIPickerView *)picker attributedTitleForRow:(NSInteger)row forComponent:(NSInteger)component {
    NSString *keys[] = {@"touch.crosshair", @"touch.joystickTap", @"touch.dpadTap"};
    return [[NSAttributedString alloc] initWithString:TouchText(keys[row])
        attributes:@{NSForegroundColorAttributeName:UIColor.whiteColor}];
}
- (void)pickerView:(UIPickerView *)picker didSelectRow:(NSInteger)row inComponent:(NSInteger)component {
    horizon_ios_touch_command(10, row);
}
- (void)options:(const HorizonTouchFrame *)f {
    UIView *panel = [UIView new];
    panel.translatesAutoresizingMaskIntoConstraints = NO;
    panel.backgroundColor = [UIColor colorWithWhite:0.07 alpha:0.98];
    panel.layer.cornerRadius = 12;
    self.panel = panel;
    [self.surface addSubview:panel];
    UILayoutGuide *safe = self.surface.safeAreaLayoutGuide;
    [NSLayoutConstraint activateConstraints:@[
        [panel.centerXAnchor constraintEqualToAnchor:safe.centerXAnchor],
        [panel.topAnchor constraintEqualToAnchor:safe.topAnchor constant:6],
        [panel.bottomAnchor constraintEqualToAnchor:safe.bottomAnchor constant:-6],
        [panel.widthAnchor constraintLessThanOrEqualToConstant:560],
        [panel.widthAnchor constraintLessThanOrEqualToAnchor:safe.widthAnchor constant:-16],
    ]];
    NSLayoutConstraint *preferred = [panel.widthAnchor constraintEqualToAnchor:safe.widthAnchor constant:-16];
    preferred.priority = 750; preferred.active = YES;
    UILabel *title = [self label:@"touch.options"];
    title.font = [UIFont boldSystemFontOfSize:19];
    UIScrollView *scroll = [UIScrollView new];
    UIStackView *list = [UIStackView new];
    list.axis = UILayoutConstraintAxisVertical; list.spacing = 6;
    list.translatesAutoresizingMaskIntoConstraints = NO;
    [scroll addSubview:list];
    [NSLayoutConstraint activateConstraints:@[
        [list.topAnchor constraintEqualToAnchor:scroll.contentLayoutGuide.topAnchor],
        [list.bottomAnchor constraintEqualToAnchor:scroll.contentLayoutGuide.bottomAnchor],
        [list.leadingAnchor constraintEqualToAnchor:scroll.contentLayoutGuide.leadingAnchor],
        [list.trailingAnchor constraintEqualToAnchor:scroll.contentLayoutGuide.trailingAnchor],
        [list.widthAnchor constraintEqualToAnchor:scroll.frameLayoutGuide.widthAnchor]
    ]];
    NSString *keys[] = {@"touch.scheme", @"touch.sensitivity", @"touch.invert", @"touch.deadzone",
        @"touch.dynamic", nil, @"touch.size", @"touch.opacity", @"touch.left"};
    UIPickerView *scheme = [UIPickerView new];
    scheme.accessibilityLabel = TouchText(keys[0]);
    scheme.dataSource = self;
    scheme.delegate = self;
    [scheme selectRow:f->scheme inComponent:0 animated:NO];
    [scheme.heightAnchor constraintEqualToConstant:90].active = YES;
    [list addArrangedSubview:[self label:keys[0]]]; [list addArrangedSubview:scheme];
    for (NSUInteger i = 1; i < 9; i++) {
        if (!keys[i]) { continue; }
        UIControl *control;
        if (i == 2 || i == 4 || i == 8) {
            UISwitch *toggle = [UISwitch new]; toggle.on = f->values[i] != 0;
            toggle.tag = 10 + i;
            [toggle addTarget:self action:@selector(value:) forControlEvents:UIControlEventValueChanged];
            control = toggle;
        } else {
            float minimum = i == 6 ? .75 : i == 7 ? .3 : i == 1 ? .1 : 0;
            float maximum = i == 6 ? 1.4 : i == 3 ? .45 : 1;
            control = [self slider:(uint32_t)(10+i) min:minimum max:maximum current:f->values[i]];
        }
        [list addArrangedSubview:[self row:keys[i] control:control]];
    }
    UIStackView *footer = [[UIStackView alloc] initWithArrangedSubviews:@[
        [self button:@"touch.cancel" command:3], [self button:@"touch.save" command:2]]];
    footer.distribution = UIStackViewDistributionFillEqually;
    [footer.heightAnchor constraintEqualToConstant:44].active = YES;
    UIStackView *body = [[UIStackView alloc] initWithArrangedSubviews:@[title,scroll,footer]];
    body.translatesAutoresizingMaskIntoConstraints = NO; body.axis = UILayoutConstraintAxisVertical; body.spacing = 8;
    [panel addSubview:body];
    [NSLayoutConstraint activateConstraints:@[
        [body.leadingAnchor constraintEqualToAnchor:panel.leadingAnchor constant:12],
        [body.trailingAnchor constraintEqualToAnchor:panel.trailingAnchor constant:-12],
        [body.topAnchor constraintEqualToAnchor:panel.topAnchor constant:8],
        [body.bottomAnchor constraintEqualToAnchor:panel.bottomAnchor constant:-4]
    ]];
}
- (void)editor {
    self.buttonSize = [self slider:20 min:.75 max:1.4 current:1];
    self.buttonOpacity = [self slider:21 min:.4 max:1 current:1];
    self.buttonSize.accessibilityLabel = TouchText(@"touch.size");
    self.buttonOpacity.accessibilityLabel = TouchText(@"touch.opacity");
    self.reset = [self button:@"touch.reset" command:22];
    UIStackView *size = [self row:@"touch.size" control:self.buttonSize];
    UIStackView *opacity = [self row:@"touch.opacity" control:self.buttonOpacity];
    for (UIStackView *row in @[size, opacity]) {
        row.spacing = 3;
        row.backgroundColor = [UIColor colorWithWhite:0.08 alpha:0.92];
        row.layer.cornerRadius = 6;
        row.layoutMarginsRelativeArrangement = YES;
        row.layoutMargins = UIEdgeInsetsMake(0,4,0,4);
        UILabel *label = (UILabel *)row.arrangedSubviews.firstObject;
        label.font = [UIFont systemFontOfSize:12];
        label.adjustsFontSizeToFitWidth = YES;
        label.minimumScaleFactor = .65;
        label.numberOfLines = 1;
    }
    self.toolbar = [[UIStackView alloc] initWithArrangedSubviews:@[
        [self button:@"touch.cancel" command:3], size, opacity, self.reset,
        [self button:@"touch.resetAll" command:23], [self button:@"touch.save" command:2]]];
    self.toolbar.spacing = 4; self.toolbar.translatesAutoresizingMaskIntoConstraints = NO;
    [self.surface addSubview:self.toolbar];
    UILayoutGuide *safe = self.surface.safeAreaLayoutGuide;
    [NSLayoutConstraint activateConstraints:@[
        [self.toolbar.topAnchor constraintEqualToAnchor:safe.topAnchor constant:2],
        [self.toolbar.leadingAnchor constraintEqualToAnchor:safe.leadingAnchor constant:4],
        [self.toolbar.trailingAnchor constraintEqualToAnchor:safe.trailingAnchor constant:-4],
        [self.toolbar.heightAnchor constraintEqualToConstant:44]
    ]];
}
- (void)update:(const HorizonTouchFrame *)f {
    if (self.mode != f->mode) {
        self.mode = f->mode;
        [self.launcher removeFromSuperview]; self.launcher = nil;
        [self.panel removeFromSuperview]; self.panel = nil;
        [self.toolbar removeFromSuperview]; self.toolbar = nil;
        if (f->mode == 2 || f->mode == 5) {
            self.launcher = f->mode == 2
                ? [self button:@"touch.customize" command:1] : [self button:@"touch.options" command:0];
            self.launcher.translatesAutoresizingMaskIntoConstraints = NO;
            [self.surface addSubview:self.launcher];
            UILayoutGuide *safe = self.surface.safeAreaLayoutGuide;
            [NSLayoutConstraint activateConstraints:@[
                [self.launcher.leadingAnchor constraintEqualToAnchor:safe.leadingAnchor constant:60],
                [self.launcher.topAnchor constraintEqualToAnchor:safe.topAnchor constant:4],
                [self.launcher.heightAnchor constraintEqualToConstant:44],
                [self.launcher.widthAnchor constraintGreaterThanOrEqualToConstant:120]
            ]];
        } else if (f->mode == 3) { [self options:f]; }
        else if (f->mode == 4) { [self editor]; }
    }
    for (NSUInteger i = 0; i < TouchCount; i++) {
        HorizonTouchTile *tile = self.tiles[i];
        HorizonTouchButton b = f->buttons[i];
        tile.hidden = b.side <= 0;
        tile.isAccessibilityElement = f->mode == 1 && !tile.hidden && (i == 8 || i == 9);
        if (tile.hidden) { continue; }
        CGRect frame = CGRectMake(b.x,b.y,b.side,b.side);
        if (!CGRectEqualToRect(tile.frame,frame)) { tile.frame = frame; }
        if (tile.alpha != b.opacity) { tile.alpha = b.opacity; }
        CGPoint knob = i == 0 ? CGPointMake(f->knob[0],f->knob[1]) : CGPointZero;
        BOOL pad = f->scheme == 2, selected = f->selected == (int)i;
        BOOL flying = f->flying && (i == 1 || i == 2);
        if (tile.pad != pad || tile.pressed != b.down || tile.flying != flying ||
            tile.selected != selected || !CGPointEqualToPoint(tile.knob,knob)) {
            tile.pad = pad; tile.pressed = b.down; tile.flying = flying;
            tile.selected = selected; tile.knob = knob;
            [tile setNeedsDisplay];
        }
    }
    if (f->mode == 4) {
        self.buttonSize.enabled = self.buttonOpacity.enabled = self.reset.enabled = f->selected >= 0;
        if (!self.buttonSize.tracking) { self.buttonSize.value = f->selection[0]; }
        if (!self.buttonOpacity.tracking) { self.buttonOpacity.value = f->selection[1]; }
    }
}
@end
