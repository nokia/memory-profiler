use std::cmp::Ordering;
use std::iter::FusedIterator;
use std::u32;

#[derive(Default)]
pub struct VecVec<K, T> {
    index: Vec<(K, u32, u32)>,
    storage: Vec<T>,
}

impl<K, T> VecVec<K, T> {
    #[inline]
    pub fn new() -> Self {
        VecVec {
            index: Vec::new(),
            storage: Vec::new(),
        }
    }

    pub fn shrink_to_fit(&mut self) {
        self.index.shrink_to_fit();
        self.storage.shrink_to_fit();
    }

    #[inline]
    pub fn insert<I: IntoIterator<Item = T>>(&mut self, key: K, iter: I) {
        let offset = self.storage.len();
        self.storage.extend(iter);
        let length = self.storage.len() - offset;
        self.index.push((key, offset as _, length as _));
    }

    #[inline]
    pub fn iter<'a>(
        &'a self,
    ) -> impl Iterator<Item = (&'a K, &'a [T])> + ExactSizeIterator + FusedIterator {
        self.index.iter().map(move |&(ref key, offset, length)| {
            (
                key,
                &self.storage[(offset as usize)..(offset + length) as usize],
            )
        })
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.index.len()
    }

    #[inline]
    pub fn get(&self, index: usize) -> (&K, &[T]) {
        let (ref key, offset, length) = self.index[index];
        let value = &self.storage[(offset as usize)..(offset + length) as usize];
        (key, value)
    }

    pub fn sort_by<F>(&mut self, mut callback: F)
    where
        F: FnMut((&K, &[T]), (&K, &[T])) -> Ordering,
    {
        let storage = &self.storage;
        self.index.sort_by(
            |&(ref l_key, l_offset, l_length), &(ref r_key, r_offset, r_length)| {
                let lhs = &storage[(l_offset as usize)..(l_offset + l_length) as usize];
                let rhs = &storage[(r_offset as usize)..(r_offset + r_length) as usize];
                return callback((l_key, lhs), (r_key, rhs));
            },
        )
    }

    #[inline]
    pub fn sort_by_key<U, F>(&mut self, mut callback: F)
    where
        F: FnMut((&K, &[T])) -> U,
        U: Ord,
    {
        self.sort_by(|lhs, rhs| {
            let lhs = callback(lhs);
            let rhs = callback(rhs);
            lhs.cmp(&rhs)
        })
    }

    pub fn reverse(&mut self) {
        self.index.reverse();
    }
}

#[derive(Default)]
pub struct DenseVecVec<T> {
    index: Vec<(u32, u32)>,
    storage: Vec<T>,
}

impl<T> DenseVecVec<T> {
    #[inline]
    pub fn new() -> Self {
        DenseVecVec {
            index: Vec::new(),
            storage: Vec::new(),
        }
    }

    #[inline]
    pub fn push<I: IntoIterator<Item = T>>(&mut self, iter: I) -> usize {
        let offset = self.storage.len();
        self.storage.extend(iter);
        let length = self.storage.len() - offset;
        let index = self.index.len();
        self.index.push((offset as _, length as _));
        index
    }

    #[inline]
    pub fn get(&self, index: usize) -> &[T] {
        let (offset, length) = self.index[index];
        &self.storage[(offset as usize)..(offset + length) as usize]
    }

    pub fn shrink_to_fit(&mut self) {
        self.index.shrink_to_fit();
        self.storage.shrink_to_fit();
    }
}

#[test]
fn test_vecvec() {
    let mut vec: VecVec<usize, usize> = VecVec::new();
    vec.insert(0, vec![1, 2, 3]);
    vec.insert(1, vec![10]);

    assert_eq!(vec.len(), 2);
    assert_eq!(vec.iter().len(), 2);
    {
        let mut iter = vec.iter();
        assert_eq!(iter.next(), Some((&0, &[1, 2, 3][..])));
        assert_eq!(iter.next(), Some((&1, &[10][..])));
        assert_eq!(iter.next(), None);
    }

    vec.insert(3, vec![100]);
    assert_eq!(vec.len(), 3);
    assert_eq!(vec.iter().len(), 3);
    {
        let mut iter = vec.iter();
        assert_eq!(iter.next(), Some((&0, &[1, 2, 3][..])));
        assert_eq!(iter.next(), Some((&1, &[10][..])));
        assert_eq!(iter.next(), Some((&3, &[100][..])));
        assert_eq!(iter.next(), None);
    }

    vec.sort_by_key(|(&key, _)| key);
    {
        let mut iter = vec.iter();
        assert_eq!(iter.next(), Some((&0, &[1, 2, 3][..])));
        assert_eq!(iter.next(), Some((&1, &[10][..])));
        assert_eq!(iter.next(), Some((&3, &[100][..])));
        assert_eq!(iter.next(), None);
    }

    vec.sort_by_key(|(&key, _)| 100 - key);
    {
        let mut iter = vec.iter();
        assert_eq!(iter.next(), Some((&3, &[100][..])));
        assert_eq!(iter.next(), Some((&1, &[10][..])));
        assert_eq!(iter.next(), Some((&0, &[1, 2, 3][..])));
        assert_eq!(iter.next(), None);
    }
}
