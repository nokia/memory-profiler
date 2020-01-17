use std::ops::{Add, Div, Mul, Sub};

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug, Hash, Readable, Writable)]
pub struct Timestamp(u64);

impl Add for Timestamp {
    type Output = Timestamp;

    #[inline]
    fn add(self, rhs: Timestamp) -> Self::Output {
        Timestamp(self.0 + rhs.0)
    }
}

impl Sub for Timestamp {
    type Output = Timestamp;

    #[inline]
    fn sub(self, rhs: Timestamp) -> Self::Output {
        Timestamp(self.0 - rhs.0)
    }
}

impl Mul<f64> for Timestamp {
    type Output = Timestamp;

    #[inline]
    fn mul(self, rhs: f64) -> Self::Output {
        Timestamp((self.0 as f64 * rhs) as u64)
    }
}

impl Div<f64> for Timestamp {
    type Output = Timestamp;

    #[inline]
    fn div(self, rhs: f64) -> Self::Output {
        Timestamp((self.0 as f64 / rhs) as u64)
    }
}

impl Timestamp {
    #[inline]
    pub fn from_secs(secs: u64) -> Self {
        Timestamp(secs * 1_000_000)
    }

    #[inline]
    pub fn from_usecs(usecs: u64) -> Self {
        Timestamp(usecs)
    }

    #[inline]
    pub fn from_timespec(secs: u64, fract_nsecs: u64) -> Self {
        Timestamp::from_usecs(secs * 1_000_000 + fract_nsecs / 1_000)
    }

    #[inline]
    pub fn min() -> Self {
        Timestamp(0)
    }

    #[inline]
    pub fn max() -> Self {
        Timestamp(-1_i64 as u64)
    }

    #[inline]
    pub fn eps() -> Self {
        Timestamp(1)
    }

    #[inline]
    pub fn as_secs(&self) -> u64 {
        self.0 / 1_000_000
    }

    #[inline]
    pub fn as_msecs(&self) -> u64 {
        self.0 / 1_000
    }

    #[inline]
    pub fn as_usecs(&self) -> u64 {
        self.0
    }

    #[inline]
    pub fn fract_nsecs(&self) -> u64 {
        (self.as_usecs() - self.as_secs() * 1_000_000) * 1000
    }
}

#[test]
fn test_timestamp() {
    let ts = Timestamp::from_timespec(333, 987_654_321);
    assert_eq!(ts.as_secs(), 333);
    assert_eq!(ts.as_msecs(), 333987);
    assert_eq!(ts.as_usecs(), 333987654);
    assert_eq!(ts.fract_nsecs(), 987_654_000);

    assert_eq!(
        ts - Timestamp::from_secs(133),
        Timestamp::from_timespec(200, 987_654_321)
    );

    assert_eq!(
        ts - Timestamp::from_usecs(654),
        Timestamp::from_timespec(333, 987_000_321)
    );

    assert_eq!(
        Timestamp::from_secs(1) - Timestamp::from_usecs(500),
        Timestamp::from_timespec(0, 999_500_000)
    );

    assert_eq!(
        Timestamp::from_timespec(1, 200_300_400) * 2.0,
        Timestamp::from_timespec(2, 400_600_000)
    );

    assert_eq!(
        Timestamp::from_timespec(2, 200_400_400) / 2.0,
        Timestamp::from_timespec(1, 100_200_000)
    );
}
