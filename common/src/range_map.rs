use std::cmp::{max, min, Ordering};
use std::iter;
use std::ops::Range;
use std::slice;

trait RangeExt<T: PartialOrd> {
    fn includes(&self, point: T) -> bool;
    fn is_outside_of(&self, range: &Range<T>) -> bool;
}

impl<T: PartialOrd> RangeExt<T> for Range<T> {
    #[inline]
    fn includes(&self, point: T) -> bool {
        point >= self.start && point < self.end
    }

    #[inline]
    fn is_outside_of(&self, range: &Range<T>) -> bool {
        self.end <= range.start || self.start >= range.end
    }
}

pub struct RangeMap<T> {
    values: Vec<(Range<u64>, T)>,
}

fn sort<T>(vec: &mut Vec<(Range<u64>, T)>) {
    vec.sort_by_key(|&(ref range, _)| (range.start, range.end));
}

impl<T> RangeMap<T> {
    pub fn new() -> Self {
        RangeMap { values: Vec::new() }
    }

    pub fn from_vec(mut values: Vec<(Range<u64>, T)>) -> Self {
        if values.is_empty() {
            return Self::new();
        }

        debug_assert!(values
            .iter()
            .all(|&(ref range, _)| range.start <= range.end));
        sort(&mut values);

        let mut map = RangeMap {
            values: Vec::with_capacity(values.len()),
        };

        let mut iter = values.into_iter();
        let mut bounds = {
            let (range, value) = iter.next().unwrap();
            map.values.push((range.clone(), value));
            range
        };

        for (range, value) in iter {
            if !range.is_outside_of(&bounds) {
                if bounds.includes(range.start) || bounds.includes(range.end) {
                    continue;
                }

                if !map
                    .values
                    .iter()
                    .all(|&(ref existing_range, _)| range.is_outside_of(existing_range))
                {
                    continue;
                }
            }

            bounds = min(bounds.start, range.start)..max(bounds.end, range.end);
            map.values.push((range, value));
        }

        map
    }

    fn get_index_linear_search(&self, key: u64) -> Option<usize> {
        for (index, &(ref range, _)) in self.values.iter().enumerate() {
            if key >= range.start && key < range.end {
                return Some(index);
            }
        }

        None
    }

    fn get_index_binary_search(&self, key: u64) -> Option<usize> {
        self.values
            .binary_search_by(|&(ref range, _)| {
                if key >= range.start && key < range.end {
                    Ordering::Equal
                } else if key < range.start {
                    Ordering::Greater
                } else {
                    Ordering::Less
                }
            })
            .ok()
    }

    pub fn get_index(&self, key: u64) -> Option<usize> {
        if self.values.len() <= 4 {
            self.get_index_linear_search(key)
        } else {
            self.get_index_binary_search(key)
        }
    }

    pub fn get_index_by_any_point(&self, range: &Range<u64>) -> Option<usize> {
        self.values
            .iter()
            .position(|&(ref existing_range, _)| !range.is_outside_of(existing_range))
    }

    #[inline]
    pub fn get_by_index(&self, index: usize) -> Option<(Range<u64>, &T)> {
        self.values
            .get(index)
            .map(|&(ref range, ref value)| (range.clone(), value))
    }

    #[inline]
    pub fn get_value_by_index(&self, index: usize) -> Option<&T> {
        self.values.get(index).map(|&(_, ref value)| value)
    }

    #[inline]
    pub fn get(&self, key: u64) -> Option<(Range<u64>, &T)> {
        if let Some(index) = self.get_index(key) {
            let entry = &self.values[index];
            Some((entry.0.clone(), &entry.1))
        } else {
            None
        }
    }

    #[inline]
    pub fn get_value(&self, key: u64) -> Option<&T> {
        self.get(key).map(|(_, value)| value)
    }

    #[inline]
    pub fn values(&self) -> iter::Map<slice::Iter<(Range<u64>, T)>, fn(&(Range<u64>, T)) -> &T> {
        self.values.iter().map(|&(_, ref value)| value)
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.values.len()
    }

    #[inline]
    pub fn push(&mut self, range: Range<u64>, value: T) -> Result<(), usize> {
        if let Some(position) = self
            .values
            .iter()
            .position(|&(ref existing_range, _)| !range.is_outside_of(existing_range))
        {
            return Err(position);
        }

        self.values.push((range, value));
        sort(&mut self.values);

        Ok(())
    }

    #[inline]
    pub fn remove_by_exact_range(&mut self, range: Range<u64>) -> Option<T> {
        if let Some(index) = self
            .values
            .iter()
            .position(|&(ref item_range, _)| range == *item_range)
        {
            Some(self.values.remove(index).1)
        } else {
            None
        }
    }

    #[inline]
    pub fn remove_by_index(&mut self, index: usize) -> (Range<u64>, T) {
        self.values.remove(index)
    }

    #[inline]
    pub fn retain<F: FnMut(&T) -> bool>(&mut self, mut callback: F) {
        self.values.retain(|&(_, ref value)| callback(value))
    }
}

impl<T> IntoIterator for RangeMap<T> {
    type Item = (Range<u64>, T);
    type IntoIter = <Vec<(Range<u64>, T)> as IntoIterator>::IntoIter;
    fn into_iter(self) -> Self::IntoIter {
        self.values.into_iter()
    }
}

#[cfg(test)]
mod tests {
    use super::RangeMap;

    #[test]
    fn basic() {
        let map = RangeMap::from_vec(vec![
            (0..10, 0),
            (100..1000, 1),
            (5000..6000, 2),
            (10000..20000, 3),
            (40000..40005, 4),
            (50000..55000, 5),
            (60000..65000, 6),
            (62500..70000, 7),
        ]);

        assert_eq!(map.get_value(0), Some(&0));
        assert_eq!(map.get_value(5), Some(&0));
        assert_eq!(map.get_value(9), Some(&0));
        assert_eq!(map.get_value(10), None);
        assert_eq!(map.get_value(100), Some(&1));
        assert_eq!(map.get_value(500), Some(&1));
        assert_eq!(map.get_value(5000), Some(&2));
        assert_eq!(map.get_value(10000), Some(&3));
        assert_eq!(map.get_value(40000), Some(&4));
        assert_eq!(map.get_value(50000), Some(&5));
        assert_eq!(map.get_value(62000), Some(&6));
        assert_eq!(map.get_value(68000), None);
    }
}
