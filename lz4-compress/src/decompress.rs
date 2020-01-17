//! The decompression algorithm.

use byteorder::{ByteOrder, LittleEndian};

quick_error! {
    /// An error representing invalid compressed data.
    #[derive(Debug)]
    pub enum Error {
        /// Expected another byte, but none found.
        ExpectedAnotherByte {
            description("Expected another byte, found none.")
        }
        /// Deduplication offset out of bounds (not in buffer).
        OffsetOutOfBounds {
            description("The offset to copy is not contained in the decompressed buffer.")
        }
    }
}

/// A LZ4 decoder.
///
/// This will decode in accordance to the LZ4 format. It represents a particular state of the
/// decompressor.
struct Decoder<'a> {
    /// The compressed input.
    input: &'a [u8],
    /// The decompressed output.
    output: &'a mut Vec<u8>,
    /// The current block's "token".
    ///
    /// This token contains to 4-bit "fields", a higher and a lower, representing the literals'
    /// length and the back reference's length, respectively. LSIC is used if either are their
    /// maximal values.
    token: u8,
}

impl<'a> Decoder<'a> {
    /// Internal (partial) function for `take`.
    #[inline]
    fn take_imp(input: &mut &'a [u8], n: usize) -> Result<&'a [u8], Error> {
        // Check if we have enough bytes left.
        if input.len() < n {
            // No extra bytes. This is clearly not expected, so we return an error.
            Err(Error::ExpectedAnotherByte)
        } else {
            // Take the first n bytes.
            let res = Ok(&input[..n]);
            // Shift the stream to left, so that it is no longer the first byte.
            *input = &input[n..];

            // Return the former first byte.
            res
        }
    }

    /// Pop n bytes from the start of the input stream.
    #[inline]
    fn take(&mut self, n: usize) -> Result<&[u8], Error> {
        Self::take_imp(&mut self.input, n)
    }

    /// Write a buffer to the output stream.
    ///
    /// The reason this doesn't take `&mut self` is that we need partial borrowing due to the rules
    /// of the borrow checker. For this reason, we instead take some number of segregated
    /// references so we can read and write them independently.
    #[inline]
    fn output(output: &mut Vec<u8>, buf: &[u8]) {
        // We use simple memcpy to extend the vector.
        output.extend_from_slice(&buf[..buf.len()]);
    }

    /// Write an already decompressed match to the output stream.
    ///
    /// This is used for the essential part of the algorithm: deduplication. We start at some
    /// position `start` and then keep pushing the following element until we've added
    /// `match_length` elements.
    #[inline]
    fn duplicate(&mut self, start: usize, match_length: usize) {
        // We cannot simply use memcpy or `extend_from_slice`, because these do not allow
        // self-referential copies: http://ticki.github.io/img/lz4_runs_encoding_diagram.svg
        self.output.reserve(match_length);
        if start + match_length > self.output.len() {
            for i in start..start + match_length {
                let b = self.output.as_slice()[i];
                self.output.push(b);
            }
        } else {
            let length = self.output.len();
            unsafe {
                self.output.set_len(length + match_length);
            }
            let (src, dst) = self.output.as_mut_slice().split_at_mut(length);
            dst.copy_from_slice(&src[start..start + match_length]);
        }
    }

    /// Read an integer LSIC (linear small integer code) encoded.
    ///
    /// In LZ4, we encode small integers in a way that we can have an arbitrary number of bytes. In
    /// particular, we add the bytes repeatedly until we hit a non-0xFF byte. When we do, we add
    /// this byte to our sum and terminate the loop.
    ///
    /// # Example
    ///
    /// ```notest
    ///     255, 255, 255, 4, 2, 3, 4, 6, 7
    /// ```
    ///
    /// is encoded to _255 + 255 + 255 + 4 = 769_. The bytes after the first 4 is ignored, because
    /// 4 is the first non-0xFF byte.
    #[inline]
    fn read_integer(&mut self) -> Result<usize, Error> {
        // We start at zero and count upwards.
        let mut n = 0;
        // If this byte takes value 255 (the maximum value it can take), another byte is read
        // and added to the sum. This repeats until a byte lower than 255 is read.
        while {
            // We add the next byte until we get a byte which we add to the counting variable.
            let extra = self.take(1)?[0];
            n += extra as usize;

            // We continue if we got 255.
            extra == 0xFF
        } {}

        Ok(n)
    }

    /// Read a little-endian 16-bit integer from the input stream.
    #[inline]
    fn read_u16(&mut self) -> Result<u16, Error> {
        // We use byteorder to read an u16 in little endian.
        Ok(LittleEndian::read_u16(self.take(2)?))
    }

    /// Read the literals section of a block.
    ///
    /// The literals section encodes some bytes which are to be copied to the output without any
    /// modification.
    ///
    /// It consists of two parts:
    ///
    /// 1. An LSIC integer extension to the literals length as defined by the first part of the
    ///    token, if it takes the highest value (15).
    /// 2. The literals themself.
    #[inline]
    fn read_literal_section(&mut self) -> Result<(), Error> {
        // The higher token is the literals part of the token. It takes a value from 0 to 15.
        let mut literal = (self.token >> 4) as usize;
        // If the initial value is 15, it is indicated that another byte will be read and added to
        // it.
        if literal == 15 {
            // The literal length took the maximal value, indicating that there is more than 15
            // literal bytes. We read the extra integer.
            literal += self.read_integer()?;
        }

        // Now we know the literal length. The number will be used to indicate how long the
        // following literal copied to the output buffer is.

        // Read the literals segment and output them without processing.
        Self::output(&mut self.output, Self::take_imp(&mut self.input, literal)?);

        Ok(())
    }

    /// Read the duplicates section of the block.
    ///
    /// The duplicates section serves to reference an already decoded segment. This consists of two
    /// parts:
    ///
    /// 1. A 16-bit little-endian integer defining the "offset", i.e. how long back we need to go
    ///    in the decoded buffer and copy.
    /// 2. An LSIC integer extension to the duplicate length as defined by the first part of the
    ///    token, if it takes the highest value (15).
    #[inline]
    fn read_duplicate_section(&mut self) -> Result<(), Error> {
        // Now, we will obtain the offset which we will use to copy from the output. It is an
        // 16-bit integer.
        let offset = self.read_u16()?;

        // Obtain the initial match length. The match length is the length of the duplicate segment
        // which will later be copied from data previously decompressed into the output buffer. The
        // initial length is derived from the second part of the token (the lower nibble), we read
        // earlier. Since having a match length of less than 4 would mean negative compression
        // ratio, we start at 4.
        let mut match_length = (4 + (self.token & 0xF)) as usize;

        // The intial match length can maximally be 19. As with the literal length, this indicates
        // that there are more bytes to read.
        if match_length == 4 + 15 {
            // The match length took the maximal value, indicating that there is more bytes. We
            // read the extra integer.
            match_length += self.read_integer()?;
        }

        // We now copy from the already decompressed buffer. This allows us for storing duplicates
        // by simply referencing the other location.

        // Calculate the start of this duplicate segment. We use wrapping subtraction to avoid
        // overflow checks, which we will catch later.
        let start = self.output.len().wrapping_sub(offset as usize);

        // We'll do a bound check to avoid panicking.
        if start < self.output.len() {
            // Write the duplicate segment to the output buffer.
            self.duplicate(start, match_length);

            Ok(())
        } else {
            Err(Error::OffsetOutOfBounds)
        }
    }

    /// Complete the decompression by reading all the blocks.
    ///
    /// # Decompressing a block
    ///
    /// Blocks consists of:
    ///  - A 1 byte token
    ///      * A 4 bit integer $t_1$.
    ///      * A 4 bit integer $t_2$.
    ///  - A $n$ byte sequence of 0xFF bytes (if $t_1 \neq 15$, then $n = 0$).
    ///  - $x$ non-0xFF 8-bit integers, L (if $t_1 = 15$, $x = 1$, else $x = 0$).
    ///  - $t_1 + 15n + L$ bytes of uncompressed data (literals).
    ///  - 16-bits offset (little endian), $a$.
    ///  - A $m$ byte sequence of 0xFF bytes (if $t_2 \neq 15$, then $m = 0$).
    ///  - $y$ non-0xFF 8-bit integers, $c$ (if $t_2 = 15$, $y = 1$, else $y = 0$).
    ///
    /// First, the literals are copied directly and unprocessed to the output buffer, then (after
    /// the involved parameters are read) $t_2 + 15m + c$ bytes are copied from the output buffer
    /// at position $a + 4$ and appended to the output buffer. Note that this copy can be
    /// overlapping.
    #[inline]
    fn complete(&mut self) -> Result<(), Error> {
        // Exhaust the decoder by reading and decompressing all blocks until the remaining buffer
        // is empty.
        while !self.input.is_empty() {
            // Read the token. The token is the first byte in a block. It is divided into two 4-bit
            // subtokens, the higher and the lower.
            self.token = self.take(1)?[0];

            // Now, we read the literals section.
            self.read_literal_section()?;

            // If the input stream is emptied, we break out of the loop. This is only the case
            // in the end of the stream, since the block is intact otherwise.
            if self.input.is_empty() {
                break;
            }

            // Now, we read the duplicates section.
            self.read_duplicate_section()?;
        }

        Ok(())
    }
}

/// Decompress all bytes of `input` into `output`.
pub fn decompress_into(input: &[u8], output: &mut Vec<u8>) -> Result<(), Error> {
    // Decode into our vector.
    Decoder {
        input: input,
        output: output,
        token: 0,
    }
    .complete()?;

    Ok(())
}

/// Decompress all bytes of `input`.
pub fn decompress(input: &[u8]) -> Result<Vec<u8>, Error> {
    // Allocate a vector to contain the decompressed stream.
    let mut vec = Vec::with_capacity(4096);

    decompress_into(input, &mut vec)?;

    Ok(vec)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn aaaaaaaaaaa_lots_of_aaaaaaaaa() {
        assert_eq!(decompress(&[0x11, b'a', 1, 0]).unwrap(), b"aaaaaa");
    }

    #[test]
    fn multiple_repeated_blocks() {
        assert_eq!(
            decompress(&[0x11, b'a', 1, 0, 0x22, b'b', b'c', 2, 0]).unwrap(),
            b"aaaaaabcbcbcbc"
        );
    }

    #[test]
    fn all_literal() {
        assert_eq!(decompress(&[0x30, b'a', b'4', b'9']).unwrap(), b"a49");
    }

    #[test]
    fn offset_oob() {
        decompress(&[0x10, b'a', 2, 0]).unwrap_err();
        decompress(&[0x40, b'a', 1, 0]).unwrap_err();
    }
}
