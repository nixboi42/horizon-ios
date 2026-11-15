//! One-time replacement of the forced build-7 defaults, per instance.
pub(super) fn defaults(
    fps: u32,
    scale: u32,
    fresh: bool,
    constrained: bool,
    migrated: bool,
) -> (u32, u32) {
    let fps = if fresh || (constrained && !migrated && fps == 30) {
        60
    } else {
        fps
    };
    let scale = if constrained && (fresh || (!migrated && scale == 100)) {
        50
    } else {
        scale
    };
    (fps, scale)
}

#[cfg(test)]
mod tests {
    use super::defaults;

    #[test]
    fn old_forced_defaults_are_replaced_once_and_later_choices_survive() {
        assert_eq!(defaults(30, 100, false, true, false), (60, 50));
        assert_eq!(defaults(30, 100, false, true, true), (30, 100));
        assert_eq!(defaults(45, 75, false, true, false), (45, 75));
        assert_eq!(defaults(30, 100, false, false, false), (30, 100));
    }

    #[test]
    fn new_instances_use_sixty_and_only_constrained_devices_reduce_scene_resolution() {
        assert_eq!(defaults(120, 100, true, true, false), (60, 50));
        assert_eq!(defaults(120, 100, true, false, false), (60, 100));
    }
}
