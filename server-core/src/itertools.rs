// This was copied from the `itertools` crate available at: https://github.com/rust-itertools/itertools

pub type SizeHint = (usize, Option<usize>);

/// Add **x** correctly to a **SizeHint**.
#[inline]
pub fn add_scalar(sh: SizeHint, x: usize) -> SizeHint {
    let (mut low, mut hi) = sh;
    low = low.saturating_add(x);
    hi = hi.and_then(|elt| elt.checked_add(x));
    (low, hi)
}

mod size_hint {
    pub use super::add_scalar;
}

#[derive(Clone, Debug)]
pub struct CoalesceCore<I>
where
    I: Iterator,
{
    iter: I,
    last: Option<I::Item>,
}

impl<I> CoalesceCore<I>
where
    I: Iterator,
{
    fn next_with<F>(&mut self, mut f: F) -> Option<I::Item>
    where
        F: FnMut(I::Item, I::Item) -> Result<I::Item, (I::Item, I::Item)>,
    {
        // this fuses the iterator
        let mut last = match self.last.take() {
            None => return None,
            Some(x) => x,
        };
        for next in &mut self.iter {
            match f(last, next) {
                Ok(joined) => last = joined,
                Err((last_, next_)) => {
                    self.last = Some(next_);
                    return Some(last_);
                }
            }
        }

        Some(last)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let (low, hi) = size_hint::add_scalar(self.iter.size_hint(), self.last.is_some() as usize);
        ((low > 0) as usize, hi)
    }
}

/// An iterator adaptor that may join together adjacent elements.
///
/// See [`.coalesce()`](../trait.Itertools.html#method.coalesce) for more information.
#[must_use = "iterator adaptors are lazy and do nothing unless consumed"]
pub struct Coalesce<I, F>
where
    I: Iterator,
{
    iter: CoalesceCore<I>,
    f: F,
}

/// Create a new `Coalesce`.
pub fn coalesce<I, F>(mut iter: I, f: F) -> Coalesce<I, F>
where
    I: Iterator,
{
    Coalesce {
        iter: CoalesceCore {
            last: iter.next(),
            iter: iter,
        },
        f: f,
    }
}

impl<I, F> Iterator for Coalesce<I, F>
where
    I: Iterator,
    F: FnMut(I::Item, I::Item) -> Result<I::Item, (I::Item, I::Item)>,
{
    type Item = I::Item;

    fn next(&mut self) -> Option<I::Item> {
        self.iter.next_with(&mut self.f)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.iter.size_hint()
    }
}

pub trait Itertools: Iterator {
    /// Return an iterator adaptor that uses the passed-in closure to
    /// optionally merge together consecutive elements.
    ///
    /// The closure `f` is passed two elements, `previous` and `current` and may
    /// return either (1) `Ok(combined)` to merge the two values or
    /// (2) `Err((previous', current'))` to indicate they can't be merged.
    /// In (2), the value `previous'` is emitted by the iterator.
    /// Either (1) `combined` or (2) `current'` becomes the previous value
    /// when coalesce continues with the next pair of elements to merge. The
    /// value that remains at the end is also emitted by the iterator.
    ///
    /// Iterator element type is `Self::Item`.
    ///
    /// This iterator is *fused*.
    fn coalesce<F>(self, f: F) -> Coalesce<Self, F>
    where
        Self: Sized,
        F: FnMut(Self::Item, Self::Item) -> Result<Self::Item, (Self::Item, Self::Item)>,
    {
        coalesce(self, f)
    }
}

impl<T: ?Sized> Itertools for T where T: Iterator {}
