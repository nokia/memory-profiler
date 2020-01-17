use hashbrown::{HashMap, HashSet};

use regex::{self, Regex};

use cli_core::{Allocation, BacktraceId, Data, Timestamp};

use crate::protocol;

#[derive(Clone, Debug)]
pub struct GroupFilter {
    pub interval_min: Option<Timestamp>,
    pub interval_max: Option<Timestamp>,
    pub leaked_allocations_min: Option<protocol::NumberOrPercentage>,
    pub leaked_allocations_max: Option<protocol::NumberOrPercentage>,
    pub allocations_min: usize,
    pub allocations_max: usize,
}

#[derive(Clone, Debug)]
pub struct Filter {
    pub timestamp_start_specified: bool,
    pub timestamp_start: Timestamp,
    pub timestamp_end_specified: bool,
    pub timestamp_end: Timestamp,
    pub address_min: u64,
    pub address_max: u64,
    pub size_min_specified: bool,
    pub size_min: u64,
    pub size_max_specified: bool,
    pub size_max: u64,
    pub lifetime_min: protocol::Interval,
    pub lifetime_max: Option<protocol::Interval>,
    pub lifetime: protocol::LifetimeFilter,
    pub backtrace_depth_min: usize,
    pub backtrace_depth_max: usize,
    pub mmaped: Option<protocol::MmapedFilter>,
    pub arena: Option<protocol::ArenaFilter>,
    pub matched_backtraces: Option<HashSet<BacktraceId>>,
    pub marker: Option<u32>,
    pub group_filter: Option<GroupFilter>,
}

impl Filter {
    pub fn timestamp_start_opt(&self) -> Option<Timestamp> {
        if self.timestamp_start_specified {
            Some(self.timestamp_start)
        } else {
            None
        }
    }

    pub fn timestamp_end_opt(&self) -> Option<Timestamp> {
        if self.timestamp_end_specified {
            Some(self.timestamp_end)
        } else {
            None
        }
    }

    pub fn size_min_opt(&self) -> Option<u64> {
        if self.size_min_specified {
            Some(self.size_min)
        } else {
            None
        }
    }

    pub fn size_max_opt(&self) -> Option<u64> {
        if self.size_max_specified {
            Some(self.size_max)
        } else {
            None
        }
    }
}

pub enum PrepareFilterError {
    InvalidRegex(&'static str, regex::Error),
}

pub fn prepare_filter(
    data: &Data,
    filter: &protocol::AllocFilter,
) -> Result<Filter, PrepareFilterError> {
    let matched_backtraces_1;
    let matched_backtraces_2;

    if filter.function_regex.is_some()
        || filter.source_regex.is_some()
        || filter.negative_function_regex.is_some()
        || filter.negative_source_regex.is_some()
    {
        let function_regex = if let Some(ref pattern) = filter.function_regex {
            Some(
                Regex::new(pattern)
                    .map_err(|err| PrepareFilterError::InvalidRegex("function_regex", err))?,
            )
        } else {
            None
        };

        let source_regex = if let Some(ref pattern) = filter.source_regex {
            Some(
                Regex::new(pattern)
                    .map_err(|err| PrepareFilterError::InvalidRegex("source_regex", err))?,
            )
        } else {
            None
        };

        let negative_function_regex =
            if let Some(ref pattern) = filter.negative_function_regex {
                Some(Regex::new(pattern).map_err(|err| {
                    PrepareFilterError::InvalidRegex("negative_function_regex", err)
                })?)
            } else {
                None
            };

        let negative_source_regex =
            if let Some(ref pattern) = filter.negative_source_regex {
                Some(Regex::new(pattern).map_err(|err| {
                    PrepareFilterError::InvalidRegex("negative_source_regex", err)
                })?)
            } else {
                None
            };

        let mut matched_backtraces = HashSet::new();
        let mut positive_cache = HashMap::new();
        let mut negative_cache = HashMap::new();
        for (backtrace_id, backtrace) in data.all_backtraces() {
            let mut positive_matched = function_regex.is_none() && source_regex.is_none();
            let mut negative_matched = false;
            let check_negative =
                negative_function_regex.is_some() || negative_source_regex.is_some();

            for (frame_id, frame) in backtrace {
                let check_positive = if positive_matched {
                    false
                } else if let Some(&cached_result) = positive_cache.get(&frame_id) {
                    positive_matched = cached_result;
                    false
                } else {
                    true
                };

                if positive_matched && !check_negative {
                    break;
                }

                let mut function = None;
                if (check_positive && function_regex.is_some()) || negative_function_regex.is_some()
                {
                    function = frame
                        .function()
                        .or_else(|| frame.raw_function())
                        .map(|id| data.interner().resolve(id).unwrap());
                }

                let mut source = None;
                if (check_positive && source_regex.is_some()) || negative_source_regex.is_some() {
                    source = frame
                        .source()
                        .map(|id| data.interner().resolve(id).unwrap())
                }

                if check_positive {
                    let matched_function = if let Some(regex) = function_regex.as_ref() {
                        if let Some(ref function) = function {
                            regex.is_match(function)
                        } else {
                            false
                        }
                    } else {
                        true
                    };

                    let matched_source = if let Some(regex) = source_regex.as_ref() {
                        if let Some(ref source) = source {
                            regex.is_match(source)
                        } else {
                            false
                        }
                    } else {
                        true
                    };

                    positive_matched = matched_function && matched_source;
                    positive_cache.insert(frame_id, positive_matched);
                }

                if check_negative {
                    match negative_cache.get(&frame_id).cloned() {
                        Some(true) => {
                            negative_matched = true;
                            break;
                        }
                        Some(false) => {
                            continue;
                        }
                        None => {}
                    }

                    if let Some(regex) = negative_function_regex.as_ref() {
                        if let Some(ref function) = function {
                            if regex.is_match(function) {
                                negative_cache.insert(frame_id, true);
                                negative_matched = true;
                                break;
                            }
                        }
                    }

                    if let Some(regex) = negative_source_regex.as_ref() {
                        if let Some(ref source) = source {
                            if regex.is_match(source) {
                                negative_cache.insert(frame_id, true);
                                negative_matched = true;
                                break;
                            }
                        }
                    }

                    negative_cache.insert(frame_id, false);
                }
            }

            if positive_matched && !negative_matched {
                matched_backtraces.insert(backtrace_id);
            }
        }

        matched_backtraces_1 = Some(matched_backtraces);
    } else {
        matched_backtraces_1 = None;
    }

    if let Some(backtrace) = filter.backtraces {
        let mut matched_backtraces = HashSet::new();
        matched_backtraces.insert(BacktraceId::new(backtrace));
        matched_backtraces_2 = Some(matched_backtraces);
    } else {
        matched_backtraces_2 = None;
    }

    let matched_backtraces = match (matched_backtraces_1, matched_backtraces_2) {
        (None, None) => None,
        (Some(left), None) => Some(left),
        (None, Some(right)) => Some(right),
        (Some(left), Some(right)) => Some(left.intersection(&right).cloned().collect()),
    };

    let group_interval_min = filter
        .group_interval_min
        .map(|ts| ts.to_timestamp(data.initial_timestamp(), data.last_timestamp()));
    let group_interval_max = filter
        .group_interval_max
        .map(|ts| ts.to_timestamp(data.initial_timestamp(), data.last_timestamp()));

    let has_group_filter = group_interval_min.is_some()
        || group_interval_max.is_some()
        || filter.group_leaked_allocations_min.is_some()
        || filter.group_leaked_allocations_max.is_some()
        || filter.group_allocations_min.is_some()
        || filter.group_allocations_max.is_some();

    let group_filter = if has_group_filter {
        let group_filter = GroupFilter {
            interval_min: group_interval_min,
            interval_max: group_interval_max,
            leaked_allocations_min: filter.group_leaked_allocations_min,
            leaked_allocations_max: filter.group_leaked_allocations_max,
            allocations_min: filter
                .group_allocations_min
                .map(|value| value as usize)
                .unwrap_or(0),
            allocations_max: filter
                .group_allocations_max
                .map(|value| value as usize)
                .unwrap_or(-1_i32 as _),
        };
        Some(group_filter)
    } else {
        None
    };

    let filter = Filter {
        timestamp_start_specified: filter.from.is_some(),
        timestamp_start: filter
            .from
            .map(|ts| ts.to_timestamp(data.initial_timestamp(), data.last_timestamp()))
            .unwrap_or(Timestamp::min()),
        timestamp_end_specified: filter.to.is_some(),
        timestamp_end: filter
            .to
            .map(|ts| ts.to_timestamp(data.initial_timestamp(), data.last_timestamp()))
            .unwrap_or(Timestamp::max()),
        address_min: filter.address_min.unwrap_or(0),
        address_max: filter.address_max.unwrap_or(-1_i32 as _),
        size_min_specified: filter.size_min.is_some(),
        size_min: filter.size_min.unwrap_or(0),
        size_max_specified: filter.size_max.is_some(),
        size_max: filter.size_max.unwrap_or(-1_i32 as _),
        lifetime_min: filter.lifetime_min.unwrap_or(protocol::Interval::min()),
        lifetime_max: filter.lifetime_max,
        lifetime: filter.lifetime.unwrap_or(protocol::LifetimeFilter::All),
        backtrace_depth_min: filter.backtrace_depth_min.unwrap_or(0) as _,
        backtrace_depth_max: filter.backtrace_depth_max.unwrap_or(-1_i32 as _) as _,
        mmaped: filter.mmaped,
        arena: filter.arena,
        matched_backtraces,
        marker: filter.marker,
        group_filter,
    };

    Ok(filter)
}

#[inline]
pub fn match_allocation(data: &Data, allocation: &Allocation, filter: &Filter) -> bool {
    let timestamp_start = filter.timestamp_start;
    let timestamp_end = filter.timestamp_end;
    let size_min = filter.size_min;
    let size_max = filter.size_max;
    let lifetime_min = filter.lifetime_min;
    let lifetime_max = filter.lifetime_max;
    let backtrace_depth_min = filter.backtrace_depth_min;
    let backtrace_depth_max = filter.backtrace_depth_max;

    if allocation.pointer < filter.address_min {
        return false;
    }

    if allocation.pointer > filter.address_max {
        return false;
    }

    if allocation.timestamp < timestamp_start {
        return false;
    }

    if allocation.timestamp > timestamp_end {
        return false;
    }

    if allocation.size < size_min || allocation.size > size_max {
        return false;
    }

    match filter.lifetime {
        protocol::LifetimeFilter::All => {}
        protocol::LifetimeFilter::OnlyLeaked => {
            if allocation.deallocation.is_some() {
                return false;
            }
        }
        protocol::LifetimeFilter::OnlyNotDeallocatedInCurrentRange => {
            if let Some(ref deallocation) = allocation.deallocation {
                if deallocation.timestamp <= timestamp_end {
                    return false;
                }
            }
        }
        protocol::LifetimeFilter::OnlyDeallocatedInCurrentRange => {
            if let Some(ref deallocation) = allocation.deallocation {
                if deallocation.timestamp > timestamp_end {
                    return false;
                }
            } else {
                return false;
            }
        }
        protocol::LifetimeFilter::OnlyTemporary => {
            if allocation.deallocation.is_none() {
                return false;
            }
        }
        protocol::LifetimeFilter::OnlyWholeGroupLeaked => {
            if allocation.deallocation.is_some() {
                return false;
            }

            let stats = data.get_group_statistics(allocation.backtrace);
            if stats.free_count != 0 {
                return false;
            }
        }
    }

    let backtrace = data.get_backtrace(allocation.backtrace);
    if backtrace.len() < backtrace_depth_min || backtrace.len() > backtrace_depth_max {
        return false;
    }

    if let Some(ref deallocation) = allocation.deallocation {
        let lifetime = deallocation.timestamp - allocation.timestamp;
        if lifetime < lifetime_min.0 {
            return false;
        }

        if let Some(lifetime_max) = lifetime_max {
            if lifetime > lifetime_max.0 {
                return false;
            }
        }
    } else {
        if lifetime_max.is_some() {
            return false;
        }
    }

    if let Some(mmaped) = filter.mmaped {
        let ok = match mmaped {
            protocol::MmapedFilter::Yes => allocation.is_mmaped(),
            protocol::MmapedFilter::No => !allocation.is_mmaped(),
        };
        if !ok {
            return false;
        }
    }

    if let Some(arena) = filter.arena {
        let ok = match arena {
            protocol::ArenaFilter::Main => !allocation.in_non_main_arena(),
            protocol::ArenaFilter::NonMain => allocation.in_non_main_arena(),
        };
        if !ok {
            return false;
        }
    }

    if let Some(marker) = filter.marker {
        if allocation.marker != marker {
            return false;
        }
    }

    if let Some(ref matched_backtraces) = filter.matched_backtraces {
        if !matched_backtraces.contains(&allocation.backtrace) {
            return false;
        }
    }

    if let Some(ref group_filter) = filter.group_filter {
        let group_allocations = data.get_allocation_ids_by_backtrace(allocation.backtrace);
        if group_allocations.len() < group_filter.allocations_min {
            return false;
        }

        if group_allocations.len() > group_filter.allocations_max {
            return false;
        }

        let first_timestamp = data
            .get_allocation(*group_allocations.first().unwrap())
            .timestamp;
        let last_timestamp = data
            .get_allocation(*group_allocations.last().unwrap())
            .timestamp;
        let interval = last_timestamp - first_timestamp;

        if interval < group_filter.interval_min.unwrap_or(Timestamp::min()) {
            return false;
        }

        if interval > group_filter.interval_max.unwrap_or(Timestamp::max()) {
            return false;
        }

        let stats = data.get_group_statistics(allocation.backtrace);
        let total_allocations = stats.alloc_count as u32;
        let leaked = (stats.alloc_count - stats.free_count) as u32;

        let leaked_min = group_filter
            .leaked_allocations_min
            .map(|threshold| threshold.get(total_allocations))
            .unwrap_or(0);
        let leaked_max = group_filter
            .leaked_allocations_max
            .map(|threshold| threshold.get(total_allocations))
            .unwrap_or(-1_i32 as _);

        if leaked < leaked_min {
            return false;
        }

        if leaked > leaked_max {
            return false;
        }
    }

    true
}
