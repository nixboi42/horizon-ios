# Touch controls for Horizon iOS 0.2.0

The default scheme is **Joystick & aim crosshair**. The movement stick is on the
left, jump and combat are on the right, and chat and pause are above the world.
Use **Options → Touch Controls** to choose **Joystick & tap to interact** or
**D-pad & tap to interact** and to tune sensitivity, inversion, deadzone, stick
position, control size, opacity, and handedness.

In either tap scheme, drag the world to turn, tap a block to use/place, tap an
entity to attack, or hold a stationary finger to mine. The use button also works
in these schemes for held items, such as food and bows. Tap Sprint or Crouch
to turn it on, then tap again to turn it off. Jump and crouch also control
swimming and flight. Double-tap jump with
flight permission to enter or leave creative flight.

**Pause → Customize Controls** previews the current scheme over the paused
world. Drag a control and use the Size and Opacity sliders for the selected
control. Use Reset for one item, Reset All for the current scheme, and Save to
apply the draft. Cancel discards it. Options and all three layouts are stored in
application support beside the instance directories. Layouts use normalized
safe-area positions and avoid covering other controls or the vanilla hotbar
and status area. A crowded layout may require adjusting a control's placement
or reducing its displayed size to keep it reachable.

The Rust host owns touch gestures and hit testing. UIKit composites the same
control rectangles using original vector shapes; interactive world touches
still go through winit. The touch path uses a fixed finger table and sends UIKit
a fixed size snapshot only when the appearance changes. Layout search runs on
geometry or settings changes. No touch graphics are included in the resource
pack, and the game continues drawing at the device's native resolution.

This is a Bedrock-inspired HUD over a Java Edition client. Interaction reach,
combat, use behavior, flight, and fluid physics remain Java Edition behavior.
Analogue movement is available in local worlds; remote Java servers retain
the baseline digital movement fallback for anticheat compatibility. Third-person
screen targeting uses the rendered orientation and starts interaction rays at
the player's eyes, so parallax can differ from the point shown on screen. The HUD
does not include Bedrock's touch target ring or all vehicle-specific controls.

For local validation, archive with the downstream builder and sign a copy of
the resulting app under a development bundle identifier. This leaves the
installed distribution app and its data container alone. From this checkout:

```sh
./build.sh --candidate --offline --check
./build.sh --testflight-build 8 --testflight-unsigned --offline
```

Manual checks: create a disposable launcher instance; exercise each scheme in
a new local world, including holding movement and jump while rotating and
attacking. Check taps beside the crosshair, drag across a button without
activating it, verify hotbar selection, try flight and swimming, and open and
close the chat keyboard. Move, resize, and fade a control; verify Cancel,
Reset and Save, then relaunch and check that the layout persists in a second
instance. Repeat Play/Quit and test a Java server separately.
