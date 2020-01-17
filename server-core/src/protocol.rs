use std::borrow::Cow;
use std::fmt;
use std::marker::PhantomData;
use std::str::FromStr;

use cli_core::Timestamp;
use serde::Serialize;

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize, Debug, Hash)]
#[serde(transparent)]
pub struct Secs(u64);

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize, Debug, Hash)]
#[serde(transparent)]
pub struct FractNanos(u32);

impl From<Secs> for Timestamp {
    #[inline]
    fn from(value: Secs) -> Self {
        Timestamp::from_secs(value.0)
    }
}

impl From<Timestamp> for Secs {
    #[inline]
    fn from(value: Timestamp) -> Self {
        Secs(value.as_secs())
    }
}

impl From<Timestamp> for FractNanos {
    #[inline]
    fn from(value: Timestamp) -> Self {
        FractNanos(value.fract_nsecs() as _)
    }
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct Timeval {
    pub secs: Secs,
    pub fract_nsecs: FractNanos,
}

impl From<Timestamp> for Timeval {
    #[inline]
    fn from(value: Timestamp) -> Self {
        Timeval {
            secs: value.into(),
            fract_nsecs: value.into(),
        }
    }
}

#[derive(Serialize)]
pub struct ResponseMetadata {
    pub id: String,
    pub executable: String,
    pub architecture: String,
    pub final_allocated: u64,
    pub final_allocated_count: u64,
    pub runtime: Timeval,
    pub unique_backtrace_count: u64,
    pub maximum_backtrace_depth: u32,
    pub timestamp: Timeval,
}

#[derive(Serialize)]
pub struct ResponseTimeline {
    pub xs: Vec<u64>,
    pub size_delta: Vec<i64>,
    pub count_delta: Vec<i64>,
    pub allocated_size: Vec<u64>,
    pub allocated_count: Vec<u64>,
    pub leaked_size: Vec<u64>,
    pub leaked_count: Vec<u64>,
    pub allocations: Vec<u32>,
    pub deallocations: Vec<u32>,
}

#[derive(Serialize)]
pub struct ResponseFragmentationTimeline {
    pub xs: Vec<u64>,
    pub fragmentation: Vec<u64>,
}

#[derive(Serialize)]
pub struct Frame<'a> {
    pub address: u64,
    pub address_s: String,
    pub count: u64,
    pub library: Option<&'a str>,
    pub function: Option<Cow<'a, str>>,
    pub raw_function: Option<&'a str>,
    pub source: Option<&'a str>,
    pub line: Option<u32>,
    pub column: Option<u32>,
    pub is_inline: bool,
}

#[derive(Serialize)]
pub struct ResponseBacktrace<'a> {
    pub frames: Vec<Frame<'a>>,
}

#[derive(Serialize)]
pub struct Deallocation {
    pub timestamp: Timeval,
    pub thread: u32,
}

#[derive(Serialize)]
pub struct Allocation<'a> {
    pub address: u64,
    pub address_s: String,
    pub timestamp: Timeval,
    pub timestamp_relative: Timeval,
    pub timestamp_relative_p: f32,
    pub thread: u32,
    pub size: u64,
    pub backtrace_id: u32,
    pub deallocation: Option<Deallocation>,
    pub backtrace: Vec<Frame<'a>>,
    pub is_mmaped: bool,
    pub in_main_arena: bool,
    pub extra_space: u32,
}

#[derive(Serialize)]
pub struct AllocationGroupData {
    pub size: u64,
    pub min_size: u64,
    pub max_size: u64,
    pub min_timestamp: Timeval,
    pub min_timestamp_relative: Timeval,
    pub min_timestamp_relative_p: f32,
    pub max_timestamp: Timeval,
    pub max_timestamp_relative: Timeval,
    pub max_timestamp_relative_p: f32,
    pub interval: Timeval,
    pub leaked_count: u64,
    pub allocated_count: u64,
}

#[derive(Serialize)]
pub struct AllocationGroup<'a> {
    pub all: AllocationGroupData,
    pub only_matched: AllocationGroupData,
    pub backtrace_id: u32,
    pub backtrace: Vec<Frame<'a>>,
}

#[derive(Serialize)]
pub struct Mallopt<'a> {
    pub timestamp: Timeval,
    pub thread: u32,
    pub backtrace_id: u32,
    pub backtrace: Vec<Frame<'a>>,
    pub raw_param: i32,
    pub param: Option<String>,
    pub value: i32,
    pub result: i32,
}

#[derive(Serialize)]
pub enum MmapOperation<'a> {
    #[serde(rename = "mmap")]
    Mmap {
        timestamp: Timeval,
        pointer: u64,
        pointer_s: String,
        length: u64,
        backtrace_id: u32,
        backtrace: Vec<Frame<'a>>,
        requested_address: u64,
        requested_address_s: String,
        is_readable: bool,
        is_writable: bool,
        is_executable: bool,
        is_semaphore: bool,
        grows_down: bool,
        grows_up: bool,
        is_shared: bool,
        is_private: bool,
        is_fixed: bool,
        is_anonymous: bool,
        is_uninitialized: bool,
        offset: u64,
        file_descriptor: i32,
        thread: u32,
    },
    #[serde(rename = "munmap")]
    Munmap {
        timestamp: Timeval,
        pointer: u64,
        pointer_s: String,
        length: u64,
        backtrace_id: u32,
        backtrace: Vec<Frame<'a>>,
        thread: u32,
    },
}

#[derive(Serialize)]
pub struct ResponseAllocations<T: Serialize> {
    pub allocations: T,
    pub total_count: u64,
}

#[derive(Serialize)]
pub struct ResponseAllocationGroups<T: Serialize> {
    pub allocations: T,
    pub total_count: u64,
}

#[derive(Serialize)]
pub struct ResponseMmaps<T: Serialize> {
    pub operations: T,
}

#[derive(Serialize)]
pub struct ResponseRegions<T: Serialize> {
    pub main_heap_start: u64,
    pub main_heap_end: u64,
    pub main_heap_start_s: String,
    pub main_heap_end_s: String,
    pub regions: T,
}

#[derive(Copy, Clone, PartialEq, Eq, Deserialize, Debug, Hash)]
pub enum LifetimeFilter {
    #[serde(rename = "all")]
    All,
    #[serde(rename = "only_leaked")]
    OnlyLeaked,
    #[serde(rename = "only_not_deallocated_in_current_range")]
    OnlyNotDeallocatedInCurrentRange,
    #[serde(rename = "only_deallocated_in_current_range")]
    OnlyDeallocatedInCurrentRange,
    #[serde(rename = "only_temporary")]
    OnlyTemporary,
    #[serde(rename = "only_whole_group_leaked")]
    OnlyWholeGroupLeaked,
}

#[derive(Copy, Clone, PartialEq, Eq, Deserialize, Debug, Hash)]
pub enum MmapedFilter {
    #[serde(rename = "yes")]
    Yes,
    #[serde(rename = "no")]
    No,
}

#[derive(Copy, Clone, PartialEq, Eq, Deserialize, Debug, Hash)]
pub enum ArenaFilter {
    #[serde(rename = "main")]
    Main,
    #[serde(rename = "non_main")]
    NonMain,
}

#[derive(Copy, Clone, Deserialize, Debug)]
pub enum AllocSortBy {
    #[serde(rename = "timestamp")]
    Timestamp,
    #[serde(rename = "address")]
    Address,
    #[serde(rename = "size")]
    Size,
}

#[derive(Copy, Clone, PartialEq, Eq, Deserialize, Debug, Hash)]
pub enum AllocGroupsSortBy {
    #[serde(rename = "only_matched.min_timestamp")]
    MinTimestamp,
    #[serde(rename = "only_matched.max_timestamp")]
    MaxTimestamp,
    #[serde(rename = "only_matched.interval")]
    Interval,
    #[serde(rename = "only_matched.allocated_count")]
    AllocatedCount,
    #[serde(rename = "only_matched.leaked_count")]
    LeakedCount,
    #[serde(rename = "only_matched.size")]
    Size,

    #[serde(rename = "all.min_timestamp")]
    GlobalMinTimestamp,
    #[serde(rename = "all.max_timestamp")]
    GlobalMaxTimestamp,
    #[serde(rename = "all.interval")]
    GlobalInterval,
    #[serde(rename = "all.allocated_count")]
    GlobalAllocatedCount,
    #[serde(rename = "all.leaked_count")]
    GlobalLeakedCount,
    #[serde(rename = "all.size")]
    GlobalSize,
}

impl Default for AllocSortBy {
    fn default() -> Self {
        AllocSortBy::Timestamp
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Deserialize, Debug, Hash)]
pub enum Order {
    #[serde(rename = "asc")]
    Asc,
    #[serde(rename = "dsc")]
    Dsc,
}

impl Default for Order {
    fn default() -> Self {
        Order::Asc
    }
}

fn get_while<'a>(p: &mut &'a str, callback: impl Fn(char) -> bool) -> &'a str {
    let mut found = None;
    for (index, ch) in p.char_indices() {
        if !callback(ch) {
            found = Some(index);
            break;
        }
    }

    if let Some(index) = found {
        let (before, after) = p.split_at(index);
        *p = after;
        before
    } else {
        let before = *p;
        *p = "";
        before
    }
}

#[derive(Debug)]
pub struct IntervalParseError;

#[derive(Copy, Clone, PartialEq, Eq, Debug, Hash)]
pub struct Interval(pub Timestamp);

impl Interval {
    pub fn min() -> Self {
        Interval(Timestamp::min())
    }
}

impl FromStr for Interval {
    type Err = IntervalParseError;

    fn from_str(string: &str) -> Result<Self, Self::Err> {
        let mut timestamp = Timestamp::min();
        let string = string.replace(" ", "");
        let mut string = string.as_str();
        while !string.is_empty() {
            let number = get_while(&mut string, |ch| ch.is_digit(10) || ch == ' ');
            let unit = get_while(&mut string, |ch| ch.is_alphabetic() || ch == ' ');
            if number.is_empty() || (unit.is_empty() && !string.is_empty()) {
                return Err(IntervalParseError);
            }
            let unit = match unit {
                "h" | "H" => Timestamp::from_secs(3600),
                "m" | "M" => Timestamp::from_secs(60),
                "s" | "S" | "" => Timestamp::from_secs(1),
                "ms" | "MS" | "Ms" | "mS" => Timestamp::from_usecs(1000),
                "us" | "US" | "Us" | "uS" => Timestamp::from_usecs(1),
                _ => return Err(IntervalParseError),
            };
            let number: u64 = number.parse().map_err(|_| IntervalParseError)?;
            let number = number as f64;
            timestamp = timestamp + (unit * number);
        }

        Ok(Interval(timestamp))
    }
}

#[test]
fn test_parse_interval() {
    fn assert(string: &str, ts: Timestamp) {
        let x: Interval = string.parse().unwrap();
        assert_eq!(x.0, ts);
    }

    assert("1", Timestamp::from_secs(1));
    assert("10", Timestamp::from_secs(10));
    assert("10s", Timestamp::from_secs(10));
    assert("3m", Timestamp::from_secs(60 * 3));
    assert("3h", Timestamp::from_secs(3600 * 3));
    assert("4h3m", Timestamp::from_secs(3600 * 4 + 60 * 3));
    assert("4h3m2s", Timestamp::from_secs(3600 * 4 + 60 * 3 + 2));
    assert("4h2s", Timestamp::from_secs(3600 * 4 + 2));
    assert("1000ms", Timestamp::from_secs(1));
    assert("100ms", Timestamp::from_usecs(100_000));
    assert("100us", Timestamp::from_usecs(100));
}

impl<'de> serde::Deserialize<'de> for Interval {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = Interval;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                write!(formatter, "interval")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                let interval: Interval = value
                    .parse()
                    .map_err(|_| E::custom("not a valid interval"))?;
                Ok(interval)
            }
        }

        deserializer.deserialize_any(Visitor)
    }
}

pub trait TimevalKind {
    fn is_end_of_the_range() -> bool;
    fn is_interval() -> bool;
}

#[derive(Copy, Clone, PartialEq, Eq, Debug, Hash)]
pub struct TimestampMin;
#[derive(Copy, Clone, PartialEq, Eq, Debug, Hash)]
pub struct TimestampMax;

impl TimevalKind for TimestampMin {
    fn is_end_of_the_range() -> bool {
        false
    }
    fn is_interval() -> bool {
        false
    }
}
impl TimevalKind for TimestampMax {
    fn is_end_of_the_range() -> bool {
        true
    }
    fn is_interval() -> bool {
        false
    }
}
impl TimevalKind for Interval {
    fn is_end_of_the_range() -> bool {
        false
    }
    fn is_interval() -> bool {
        true
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug, Hash)]
pub enum TimestampFilter<K: TimevalKind> {
    Timestamp(Timestamp),
    Secs(Secs, PhantomData<K>),
    Percent(u32),
}

impl<K> TimestampFilter<K>
where
    K: TimevalKind,
{
    pub fn to_timestamp(&self, start_at: Timestamp, end_at: Timestamp) -> Timestamp {
        match *self {
            TimestampFilter::Secs(secs, _) => {
                let mut timestamp = secs.into();
                if K::is_end_of_the_range() {
                    // We need to do this since the filter is specifed
                    // in seconds while we use a higher precision timestamps
                    // internally.
                    if timestamp != Timestamp::max() {
                        timestamp = timestamp + Timestamp::from_secs(1) - Timestamp::eps();
                    }
                }
                timestamp
            }
            TimestampFilter::Timestamp(timestamp) => timestamp,
            TimestampFilter::Percent(percentage) => {
                let range = end_at - start_at;
                let p = percentage as f64 / 100.0;
                let shift = range * p;

                if K::is_interval() {
                    shift
                } else {
                    start_at + shift
                }
            }
        }
    }
}

impl<'de, K> serde::Deserialize<'de> for TimestampFilter<K>
where
    K: TimevalKind,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct Visitor<K>(PhantomData<K>);
        impl<'de, K> serde::de::Visitor<'de> for Visitor<K>
        where
            K: TimevalKind,
        {
            type Value = TimestampFilter<K>;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                write!(formatter, "timestamp or percentage")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                if value.ends_with("%") {
                    let value = value[0..value.len() - 1]
                        .parse()
                        .map_err(|_| E::custom("not a valid percentage"))?;
                    Ok(TimestampFilter::Percent(value))
                } else {
                    if K::is_interval() {
                        let value: Interval = value
                            .parse()
                            .map_err(|_| E::custom("not a valid interval"))?;
                        Ok(TimestampFilter::Timestamp(value.0))
                    } else {
                        let value: u64 =
                            value.parse().map_err(|_| E::custom("not a valid number"))?;
                        Ok(TimestampFilter::Secs(Secs(value), PhantomData))
                    }
                }
            }
        }

        deserializer.deserialize_any(Visitor(PhantomData))
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug, Hash)]
pub enum NumberOrPercentage {
    Absolute(u32),
    Percent(u32),
}

impl NumberOrPercentage {
    pub fn get(self, maximum: u32) -> u32 {
        match self {
            NumberOrPercentage::Absolute(value) => value,
            NumberOrPercentage::Percent(percent) => {
                ((percent as f32 / 100.0) * maximum as f32) as _
            }
        }
    }
}

impl<'de> serde::Deserialize<'de> for NumberOrPercentage {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = NumberOrPercentage;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                write!(formatter, "number or percentage")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                if value.ends_with("%") {
                    let value = value[0..value.len() - 1]
                        .parse()
                        .map_err(|_| E::custom("not a valid percentage"))?;
                    Ok(NumberOrPercentage::Percent(value))
                } else {
                    let value = value.parse().map_err(|_| E::custom("not a valid number"))?;
                    Ok(NumberOrPercentage::Absolute(value))
                }
            }
        }

        deserializer.deserialize_any(Visitor)
    }
}

#[derive(Clone, PartialEq, Eq, Deserialize, Debug, Hash)]
pub struct AllocFilter {
    pub from: Option<TimestampFilter<TimestampMin>>,
    pub to: Option<TimestampFilter<TimestampMax>>,
    pub lifetime: Option<LifetimeFilter>,
    pub address_min: Option<u64>,
    pub address_max: Option<u64>,
    pub size_min: Option<u64>,
    pub size_max: Option<u64>,
    pub lifetime_min: Option<Interval>,
    pub lifetime_max: Option<Interval>,
    pub backtrace_depth_min: Option<u32>,
    pub backtrace_depth_max: Option<u32>,
    pub backtraces: Option<u32>, // TODO: Support multiple.
    pub mmaped: Option<MmapedFilter>,
    pub arena: Option<ArenaFilter>,
    pub function_regex: Option<String>,
    pub source_regex: Option<String>,
    pub negative_function_regex: Option<String>,
    pub negative_source_regex: Option<String>,
    pub marker: Option<u32>,
    pub group_interval_min: Option<TimestampFilter<Interval>>,
    pub group_interval_max: Option<TimestampFilter<Interval>>,
    pub group_leaked_allocations_min: Option<NumberOrPercentage>,
    pub group_leaked_allocations_max: Option<NumberOrPercentage>,
    pub group_allocations_min: Option<u32>,
    pub group_allocations_max: Option<u32>,
}

#[derive(Clone, Deserialize, Debug)]
pub struct BacktraceFormat {
    pub strip_template_args: Option<bool>,
}

#[derive(Deserialize, Debug)]
pub struct RequestAllocations {
    pub skip: Option<u64>,
    pub count: Option<u32>,

    pub sort_by: Option<AllocSortBy>,
    pub order: Option<Order>,
}

#[derive(Deserialize, Debug)]
pub struct RequestAllocationGroups {
    pub skip: Option<u64>,
    pub count: Option<u32>,

    pub sort_by: Option<AllocGroupsSortBy>,
    pub order: Option<Order>,
}
