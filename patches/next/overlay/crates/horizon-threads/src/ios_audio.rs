//! Apple notifications carry a generation: CPAL's iOS device name is constant,
//! so its desktop device enumeration cannot detect a route or media reset.
unsafe extern "C" {
    fn horizon_ios_audio_begin();
    fn horizon_ios_audio_end();
    fn horizon_ios_audio_suspend();
    fn horizon_ios_audio_state() -> u64;
}
pub struct Session {
    state: u64,
}
impl Session {
    pub fn begin() -> Self {
        unsafe {
            horizon_ios_audio_begin();
        }
        Self {
            state: unsafe { horizon_ios_audio_state() },
        }
    }
    pub fn poll(&mut self) -> (bool, bool) {
        let state = unsafe { horizon_ios_audio_state() };
        let changed = self.state != state;
        self.state = state;
        (state & 1 != 0, changed)
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        unsafe {
            horizon_ios_audio_end();
        }
    }
}

pub fn suspend() {
    unsafe {
        horizon_ios_audio_suspend();
    }
}
