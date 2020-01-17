use crate::data::{CodePointer, StringId};
use std::num::NonZeroU32;

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Frame {
    address: CodePointer,
    count: u64,
    is_inline: bool,

    library: Option<StringId>,
    function: Option<StringId>,
    raw_function: Option<StringId>,
    source: Option<StringId>,
    line: Option<NonZeroU32>,
    column: Option<NonZeroU32>,
}

impl Frame {
    #[inline]
    pub fn new_unknown(address: CodePointer) -> Frame {
        Frame {
            address,
            count: 0,
            is_inline: false,
            library: None,
            function: None,
            raw_function: None,
            source: None,
            line: None,
            column: None,
        }
    }

    #[inline]
    pub fn address(&self) -> CodePointer {
        self.address
    }

    #[inline]
    pub fn count(&self) -> u64 {
        self.count
    }

    #[inline]
    pub fn is_inline(&self) -> bool {
        self.is_inline
    }

    #[inline]
    pub fn library(&self) -> Option<StringId> {
        self.library
    }

    #[inline]
    pub fn function(&self) -> Option<StringId> {
        self.function
    }

    #[inline]
    pub fn raw_function(&self) -> Option<StringId> {
        self.raw_function
    }

    #[inline]
    pub fn source(&self) -> Option<StringId> {
        self.source
    }

    #[inline]
    pub fn line(&self) -> Option<u32> {
        self.line.map(|line| line.get())
    }

    #[inline]
    pub fn column(&self) -> Option<u32> {
        self.column.map(|line| line.get())
    }

    pub fn set_is_inline(&mut self, value: bool) {
        self.is_inline = value;
    }

    pub fn set_library(&mut self, string_id: StringId) {
        self.library = Some(string_id);
    }

    pub fn set_function(&mut self, string_id: StringId) {
        self.function = Some(string_id);
    }

    pub fn set_raw_function(&mut self, string_id: StringId) {
        self.raw_function = Some(string_id);
    }

    pub fn set_source(&mut self, string_id: StringId) {
        self.source = Some(string_id);
    }

    pub fn set_line(&mut self, value: u32) {
        self.line = NonZeroU32::new(value);
    }

    pub fn set_column(&mut self, value: u32) {
        self.column = NonZeroU32::new(value);
    }

    #[inline]
    pub fn increment_count(&mut self, value: u64) {
        self.count += value;
    }

    pub fn any_function(&self) -> Option<StringId> {
        self.function.or(self.raw_function)
    }
}
