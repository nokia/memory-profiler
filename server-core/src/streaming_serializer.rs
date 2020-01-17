use serde::{Serialize, Serializer};

pub struct StreamingSerializer<F, R, T>
where
    F: Fn() -> R,
    R: Iterator<Item = T>,
{
    callback: F,
}

impl<F, R, T> StreamingSerializer<F, R, T>
where
    F: Fn() -> R,
    R: Iterator<Item = T>,
    T: Serialize,
{
    pub fn new(callback: F) -> Self {
        StreamingSerializer { callback }
    }
}

impl<F, R, T> Serialize for StreamingSerializer<F, R, T>
where
    F: Fn() -> R,
    R: Iterator<Item = T>,
    T: Serialize,
{
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeSeq;

        let iter = (self.callback)();
        let mut seq = serializer.serialize_seq(None)?;
        for element in iter {
            seq.serialize_element(&element)?;
        }

        seq.end()
    }
}
