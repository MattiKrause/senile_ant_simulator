pub use comp_time::*;
#[cfg(not(target_arch = "wasm32"))]
mod comp_time {
    use std::time::{Duration, SystemTime};

    pub struct Time(SystemTime);
    pub struct Timer(());

    impl Timer {
        pub fn new() -> Result<Self, String> {
            Ok(Self(()))
        }
        pub fn now(&self) -> Time {
            Time(SystemTime::now())
        }
        pub fn saturating_duration_till(&self, since: &Time) -> Duration {
            since.0.duration_since(self.now().0).unwrap_or(Duration::ZERO)
        }
    }

    impl Time {
        pub fn checked_add(&self, add: Duration) -> Option<Self> {
            self.0.checked_add(add).map(Time)
        }
        pub fn checked_sub(&self, sub: Duration) -> Option<Self> {
            self.0.checked_sub(sub).map(Time)
        }
        pub fn before(&self, other: &Self) -> bool {
            self.0 < other.0
        }
    }
}

#[cfg(target_arch = "wasm32")]
mod comp_time {
    use std::ops::Add;
    use std::time::{Duration};

    /// represents point in time in milliseconds, akin to [std::time::Instant]
    pub struct Time(f64);
    pub struct Timer(web_sys::Performance);

    impl Timer {
        pub fn new() -> Result<Self, String> {

            // use js_sys::Date::now() ?
            let window = web_sys::window().ok_or_else(|| format!("not in a window context"))?;
            let performance = window.performance().ok_or_else(|| format!("Failed to get performance object"))?;
            Ok(Self(performance))
        }
        pub fn now(&self) -> Time {
            Time(self.0.now())
        }
        pub fn saturating_duration_till(&self, since: &Time) -> Duration {
            let now: f64 = self.now().0;
            let diff = since.0 - now;
            Duration::try_from_secs_f64(diff / 1000.0).unwrap_or(Duration::ZERO)
        }
    }

    impl Time {
        pub fn checked_add(&self, add: Duration) -> Option<Self> {
            Some(Time(self.0.add(add.as_secs_f64() * 1000.0)))
        }
        pub fn checked_sub(&self, sub: Duration) -> Option<Self> {
            Some(self.0 - sub.as_secs_f64() * 1000.0).filter(|diff| diff >= &0.0).map(Time)
        }
        pub fn before(&self, other: &Self) -> bool {
            other.0 < other.0
        }
    }
}