//! Touch flight in window-relative points; no simulation or player physics.
use glam::{DVec3, Vec2};
use std::collections::BTreeMap;
use winit::event::TouchPhase;

#[derive(Clone, Copy)]
enum Control {
    Move { origin: Vec2, delta: Vec2 },
    Look(Vec2),
    Vertical(f64),
}
pub struct Flight {
    pub position: DVec3,
    pub yaw: f32,
    pub pitch: f32,
    pub sensitivity: f32,
    touches: BTreeMap<u64, Control>,
}
impl Default for Flight {
    fn default() -> Self {
        Self {
            position: DVec3::new(0., 2., -4.),
            yaw: 0.,
            pitch: 15.,
            sensitivity: 0.18,
            touches: BTreeMap::new(),
        }
    }
}
impl Flight {
    pub fn clear(&mut self) {
        self.touches.clear();
    }
    pub fn touch(
        &mut self,
        id: u64,
        phase: TouchPhase,
        location: [f64; 2],
        scale: f64,
        size: [f32; 2],
    ) {
        let p = Vec2::new((location[0] / scale) as f32, (location[1] / scale) as f32);
        match phase {
            TouchPhase::Started => {
                let control = if p.x < size[0] * 0.32 && p.y > size[1] * 0.5 {
                    Control::Move {
                        origin: p,
                        delta: Vec2::ZERO,
                    }
                } else if p.x > size[0] * 0.88 && p.y > size[1] * 0.5 {
                    Control::Vertical(if p.y < size[1] * 0.75 { 1. } else { -1. })
                } else {
                    Control::Look(p)
                };
                self.touches.insert(id, control);
            }
            TouchPhase::Moved => {
                if let Some(control) = self.touches.get_mut(&id) {
                    match control {
                        Control::Move { origin, delta } => {
                            *delta = ((p - *origin) / 60.).clamp_length_max(1.)
                        }
                        Control::Look(last) => {
                            self.yaw =
                                (self.yaw + (p.x - last.x) * self.sensitivity).rem_euclid(360.);
                            self.pitch =
                                (self.pitch + (p.y - last.y) * self.sensitivity).clamp(-89., 89.);
                            *last = p;
                        }
                        Control::Vertical(_) => {}
                    }
                }
            }
            TouchPhase::Ended | TouchPhase::Cancelled => {
                self.touches.remove(&id);
            }
        }
    }
    pub fn step(&mut self, seconds: f64) {
        let mut motion = DVec3::ZERO;
        for control in self.touches.values() {
            match control {
                Control::Move { delta, .. } => {
                    let yaw = f64::from(self.yaw).to_radians();
                    let forward = DVec3::new(-yaw.sin(), 0., yaw.cos());
                    let right = DVec3::new(-yaw.cos(), 0., -yaw.sin());
                    motion += forward * f64::from(-delta.y) + right * f64::from(delta.x);
                }
                Control::Vertical(y) => motion.y += y,
                _ => {}
            }
        }
        self.position += motion.clamp_length_max(1.) * 8. * seconds.clamp(0., 0.05);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn flight_is_time_based_and_cancellation_stops_motion() {
        let mut a = Flight::default();
        a.touches.insert(
            1,
            Control::Move {
                origin: Vec2::ZERO,
                delta: Vec2::new(0., -1.),
            },
        );
        let mut b = Flight {
            touches: a.touches.clone(),
            ..Flight::default()
        };
        for _ in 0..60 {
            a.step(1. / 60.);
        }
        for _ in 0..120 {
            b.step(1. / 120.);
        }
        assert!((a.position - b.position).length() < 1e-10);
        assert!((a.position.z - 4.).abs() < 1e-10);
        a.clear();
        let position = a.position;
        a.step(1.);
        assert_eq!(a.position, position);
    }
    #[test]
    fn multitouch_look_scales_in_points_and_cancel_releases_flight() {
        let mut a = Flight::default();
        let mut b = Flight::default();
        let size = [800., 400.];
        for (camera, scale) in [(&mut a, 1.), (&mut b, 3.)] {
            camera.touch(
                1,
                TouchPhase::Started,
                [100. * scale, 300. * scale],
                scale,
                size,
            );
            camera.touch(
                2,
                TouchPhase::Started,
                [500. * scale, 200. * scale],
                scale,
                size,
            );
            camera.touch(
                1,
                TouchPhase::Moved,
                [100. * scale, 240. * scale],
                scale,
                size,
            );
            camera.touch(
                2,
                TouchPhase::Moved,
                [550. * scale, 220. * scale],
                scale,
                size,
            );
            camera.step(0.05);
        }
        assert_eq!(a.position, b.position);
        assert!((a.yaw - 9.).abs() < 1e-5);
        assert!((a.pitch - 18.6).abs() < 1e-5);
        a.touch(1, TouchPhase::Cancelled, [100., 240.], 1., size);
        let position = a.position;
        a.step(0.05);
        assert_eq!(a.position, position);
        a.touch(3, TouchPhase::Started, [760., 240.], 1., size);
        a.step(0.05);
        assert!((a.position.y - position.y - 0.4).abs() < 1e-8);
        a.clear();
        let position = a.position;
        a.step(1.);
        assert_eq!(a.position, position);
    }
}
