use limitador::storage::wasm::{Clock};
use proxy_wasm::hostcalls::get_current_time;
use std::time::SystemTime;

pub struct RateLimitFilterClock;

impl Clock for RateLimitFilterClock {
    fn get_current_time(&self) -> SystemTime {
        get_current_time().expect("failed to get current time")
    }
}