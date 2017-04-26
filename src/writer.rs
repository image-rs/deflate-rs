use std::io::Write;
use std::{thread, io};

use byteorder::{WriteBytesExt, BigEndian};

use checksum::{Adler32Checksum, RollingChecksum};
use compress::compress_data_dynamic_n;
use compress::Flush;
use deflate_state::DeflateState;
use compression_options::CompressionOptions;
use zlib::{write_zlib_header, CompressionLevel};

/// Keep compressing until all the input has been compressed and output or the writer returns `Err`.
pub fn compress_until_done<W: Write>(mut input: &[u8],
                                     deflate_state: &mut DeflateState<W>,
                                     flush_mode: Flush)
                                     -> io::Result<()> {
    // This should only be used for flushing.
    assert!(flush_mode != Flush::None);
    loop {
        match compress_data_dynamic_n(input, deflate_state, flush_mode) {
            Ok(0) => {
                if deflate_state.output_buf().is_empty() {
                    break;
                } else {
                    // If the output buffer isn't empty, keep going until it is, as there is still
                    // data to be flushed.
                    input = &[];
                }
            }
            Ok(n) => {
                if n < input.len() {
                    input = &input[n..]
                } else {
                    input = &[];
                }
            }
            Err(e) => {
                match e.kind() {
                    // This error means that there may still be data to flush.
                    // This could possibly get stuck if the underlying writer keeps returning this
                    // error.
                    io::ErrorKind::Interrupted => (),
                    _ => return Err(e),
                }
            }
        }
    }

    debug_assert_eq!(deflate_state.bytes_written,
                     deflate_state.bytes_written_control);

    Ok(())
}

/// A DEFLATE encoder/compressor.
///
/// A struct implementing a `Write` interface that takes unencoded data and compresses it to
/// the provided writer using DEFLATE compression.
///
/// # Examples
///
/// ```
/// use std::io::Write;
///
/// use deflate::Compression;
/// use deflate::write::DeflateEncoder;
///
/// let data = b"This is some test data";
/// let mut encoder = DeflateEncoder::new(Vec::new(), Compression::Default);
/// encoder.write_all(data).unwrap();
/// let compressed_data = encoder.finish().unwrap();
/// # let _ = compressed_data;
/// ```
pub struct DeflateEncoder<W: Write> {
    // We use a box here to avoid putting the buffers on the stack
    // It's done here rather than in the structs themselves for now to
    // keep the data close in memory.
    // Option is used to allow us to implement `Drop` and `finish()` at the same time.
    deflate_state: Option<Box<DeflateState<W>>>,
}

impl<W: Write> DeflateEncoder<W> {
    /// Creates a new encoder using the provided compression options.
    pub fn new<O: Into<CompressionOptions>>(writer: W, options: O) -> DeflateEncoder<W> {
        DeflateEncoder { deflate_state: Some(Box::new(DeflateState::new(options.into(), writer))) }
    }

    /// Encode all pending data to the contained writer, consume this `ZlibEncoder`,
    /// and return the contained writer if writing succeeds.
    pub fn finish(mut self) -> io::Result<W> {
        self.output_all()?;
        // We have to move the inner state out of the encoder, and replace it with `None`
        // to let the `DeflateEncoder` drop safely.
        let state = self.deflate_state.take();
        Ok(state.unwrap().inner)
    }

    /// Resets the encoder (except the compression options), replacing the current writer
    /// with a new one, returning the old one.
    pub fn reset(&mut self, w: W) -> io::Result<W> {
        self.output_all()?;
        self.deflate_state.as_mut().unwrap().reset(w)
    }

    /// Output all pending data as if encoding is done, but without resetting anything
    fn output_all(&mut self) -> io::Result<()> {
        compress_until_done(&[], self.deflate_state.as_mut().unwrap(), Flush::Finish)
    }
}

impl<W: Write> io::Write for DeflateEncoder<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let flush_mode = self.deflate_state.as_ref().unwrap().flush_mode;
        compress_data_dynamic_n(buf, &mut self.deflate_state.as_mut().unwrap(), flush_mode)
    }

    /// Flush the encoder.
    ///
    /// This will flush the encoder, emulating the Sync flush method from Zlib.
    /// This essentially finishes the current block, and sends an additional empty stored block to
    /// the writer.
    fn flush(&mut self) -> io::Result<()> {
        compress_until_done(&[], self.deflate_state.as_mut().unwrap(), Flush::Sync)
    }
}

impl<W: Write> Drop for DeflateEncoder<W> {
    /// When the encoder is dropped, output the rest of the data.
    ///
    /// WARNING: This may silently fail if writing fails, so using this to finish encoding
    /// for writers where writing might fail is not recommended, for that call finish() instead.
    fn drop(&mut self) {
        // Not sure if implementing drop is a good idea or not, but we follow flate2 for now.
        // We only do this if we are not panicking, to avoid a double panic.
        if self.deflate_state.is_some() && !thread::panicking() {
            let _ = self.output_all();
        }
    }
}


/// A Zlib encoder/compressor.
///
/// A struct implementing a `Write` interface that takes unencoded data and compresses it to
/// the provided writer using DEFLATE compression with Zlib headers and trailers.
///
/// # Examples
///
/// ```
/// use std::io::Write;
///
/// use deflate::Compression;
/// use deflate::write::ZlibEncoder;
///
/// let data = b"This is some test data";
/// let mut encoder = ZlibEncoder::new(Vec::new(), Compression::Default);
/// encoder.write_all(data).unwrap();
/// let compressed_data = encoder.finish().unwrap();
/// # let _ = compressed_data;
/// ```
pub struct ZlibEncoder<W: Write> {
    // We use a box here to avoid putting the buffers on the stack
    // It's done here rather than in the structs themselves for now to
    // keep the data close in memory.
    // Option is used to allow us to implement `Drop` and `finish()` at the same time.
    deflate_state: Option<Box<DeflateState<W>>>,
    checksum: Adler32Checksum,
    header_written: bool,
}

impl<W: Write> ZlibEncoder<W> {
    /// Create a new `ZlibEncoder` using the provided compression options.
    pub fn new<O: Into<CompressionOptions>>(writer: W, options: O) -> ZlibEncoder<W> {
        ZlibEncoder {
            deflate_state: Some(Box::new(DeflateState::new(options.into(), writer))),
            checksum: Adler32Checksum::new(),
            header_written: false,
        }
    }

    /// Output all pending data ,including the trailer(checksum) as if encoding is done,
    /// but without resetting anything.
    fn output_all(&mut self) -> io::Result<()> {
        self.check_write_header()?;
        compress_until_done(&[], self.deflate_state.as_mut().unwrap(), Flush::Finish)?;
        self.write_trailer()
    }

    /// Encode all pending data to the contained writer, consume this `ZlibEncoder`,
    /// and return the contained writer if writing succeeds.
    pub fn finish(mut self) -> io::Result<W> {
        self.output_all()?;
        // We have to move the inner state out of the encoder, and replace it with `None`
        // to let the `DeflateEncoder` drop safely.
        let inner = self.deflate_state.take();
        Ok(inner.unwrap().inner)
    }

    /// Resets the encoder (except the compression options), replacing the current writer
    /// with a new one, returning the old one.
    pub fn reset(&mut self, writer: W) -> io::Result<W> {
        self.output_all()?;
        self.header_written = false;
        self.checksum = Adler32Checksum::new();
        self.deflate_state.as_mut().unwrap().reset(writer)
    }

    /// Check if a zlib header should be written.
    fn check_write_header(&mut self) -> io::Result<()> {
        if !self.header_written {
            write_zlib_header(&mut self.deflate_state.as_mut().unwrap().output_buf(),
                              CompressionLevel::Default)?;
            self.header_written = true;
        }
        Ok(())
    }

    /// Write the trailer, which for zlib is the Adler32 checksum.
    fn write_trailer(&mut self) -> io::Result<()> {
        let hash = self.checksum.current_hash();

        self.deflate_state
            .as_mut()
            .unwrap()
            .inner
            .write_u32::<BigEndian>(hash)
    }

    /// Return the adler32 checksum of the currently consumed data.
    pub fn checksum(&self) -> u32 {
        self.checksum.current_hash()
    }
}

impl<W: Write> io::Write for ZlibEncoder<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.check_write_header()?;
        let flush_mode = self.deflate_state.as_ref().unwrap().flush_mode;
        let res =
            compress_data_dynamic_n(buf, &mut self.deflate_state.as_mut().unwrap(), flush_mode);
        match res {
            Ok(0) => self.checksum.update_from_slice(buf),
            Ok(n) => self.checksum.update_from_slice(&buf[0..n]),
            _ => (),
        };
        res
    }

    /// Flush the encoder.
    ///
    /// This will flush the encoder, emulating the Sync flush method from Zlib.
    /// This essentially finishes the current block, and sends an additional empty stored block to
    /// the writer.
    fn flush(&mut self) -> io::Result<()> {
        compress_until_done(&[], self.deflate_state.as_mut().unwrap(), Flush::Sync)
    }
}

impl<W: Write> Drop for ZlibEncoder<W> {
    /// When the encoder is dropped, output the rest of the data.
    ///
    /// WARNING: This may silently fail if writing fails, so using this to finish encoding
    /// for writers where writing might fail is not recommended, for that call finish() instead.
    fn drop(&mut self) {
        if self.deflate_state.is_some() && !thread::panicking() {
            let _ = self.output_all();
        }
    }
}

#[cfg(feature = "gzip")]
pub mod gzip {

    use std::io::{Write, Cursor};
    use std::{thread, io};

    use super::*;

    use byteorder::{WriteBytesExt, LittleEndian};
    use gzip_header::{Crc, GzBuilder};

    /// A Gzip encoder/compressor.
    ///
    /// A struct implementing a `Write` interface that takes unencoded data and compresses it to
    /// the provided writer using DEFLATE compression with Gzip headers and trailers.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::io::Write;
    ///
    /// use deflate::Compression;
    /// use deflate::write::GzEncoder;
    ///
    /// let data = b"This is some test data";
    /// let mut encoder = GzEncoder::new(Vec::new(), Compression::Default);
    /// encoder.write_all(data).unwrap();
    /// let compressed_data = encoder.finish().unwrap();
    /// # let _ = compressed_data;
    /// ```
    pub struct GzEncoder<W: Write> {
        inner: DeflateEncoder<W>,
        checksum: Crc,
        header: Vec<u8>,
    }

    impl<W: Write> GzEncoder<W> {
        /// Create a new `GzEncoder` writing deflate-compressed data to the underlying writer when
        /// written to, wrapped in a gzip header and trailer. The header details will be blank.
        pub fn new<O: Into<CompressionOptions>>(writer: W, options: O) -> GzEncoder<W> {
            GzEncoder::from_builder(GzBuilder::new(), writer, options)
        }

        /// Create a new GzEncoder from the provided `GzBuilder`. This allows customising
        /// the detalis of the header, such as the filename and comment fields.
        pub fn from_builder<O: Into<CompressionOptions>>(builder: GzBuilder,
                                                         writer: W,
                                                         options: O)
                                                         -> GzEncoder<W> {
            GzEncoder {
                inner: DeflateEncoder::new(writer, options),
                checksum: Crc::new(),
                header: builder.into_header(),
            }
        }

        /// Write header to the output buffer if it hasn't been done yet.
        fn check_write_header(&mut self) {
            if !self.header.is_empty() {
                self.inner
                    .deflate_state
                    .as_mut()
                    .unwrap()
                    .output_buf()
                    .extend_from_slice(&self.header);
                self.header.clear();
            }
        }

        /// Output all pending data ,including the trailer(checksum + count) as if encoding is done.
        /// but without resetting anything.
        fn output_all(&mut self) -> io::Result<()> {
            self.check_write_header();
            self.inner.output_all()?;
            self.write_trailer()
        }

        /// Encode all pending data to the contained writer, consume this `GzEncoder`,
        /// and return the contained writer if writing succeeds.
        pub fn finish(mut self) -> io::Result<W> {
            self.output_all()?;
            // We have to move the inner state out of the encoder, and replace it with `None`
            // to let the `DeflateEncoder` drop safely.
            let inner = self.inner.deflate_state.take();
            Ok(inner.unwrap().inner)
        }

        /// Resets the encoder (except the compression options), replacing the current writer
        /// with a new one, returning the old one.
        pub fn reset(&mut self, writer: W) -> io::Result<W> {
            self.output_all()?;
            self.header = GzBuilder::new().into_header();
            self.checksum = Crc::new();
            self.inner
                .deflate_state
                .as_mut()
                .unwrap()
                .reset(writer)
        }

        /// Write the checksum and number of bytes mod 2^32 to the output writer.
        fn write_trailer(&mut self) -> io::Result<()> {
            let crc = self.checksum.sum();
            let amount = self.checksum.amt_as_u32();

            // We use a buffer here to make sure we don't end up writing only half the header if
            // writing fails.
            let mut buf = [0u8; 8];
            let mut temp = Cursor::new(&mut buf[..]);
            temp.write_u32::<LittleEndian>(crc).unwrap();
            temp.write_u32::<LittleEndian>(amount).unwrap();
            self.inner
                .deflate_state
                .as_mut()
                .unwrap()
                .inner
                .write_all(&temp.into_inner())
        }

        /// Get the crc32 checksum of the data comsumed so far.
        pub fn checksum(&self) -> u32 {
            self.checksum.sum()
        }
    }

    impl<W: Write> io::Write for GzEncoder<W> {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.check_write_header();
            let res = self.inner.write(buf);
            match res {
                Ok(0) => self.checksum.update(buf),
                Ok(n) => self.checksum.update(&buf[0..n]),
                _ => (),
            };
            res
        }

        /// Flush the encoder.
        ///
        /// This will flush the encoder, emulating the Sync flush method from Zlib.
        /// This essentially finishes the current block, and sends an additional empty stored
        /// block to the writer.
        fn flush(&mut self) -> io::Result<()> {
            self.inner.flush()
        }
    }

    impl<W: Write> Drop for GzEncoder<W> {
        /// When the encoder is dropped, output the rest of the data.
        ///
        /// WARNING: This may silently fail if writing fails, so using this to finish encoding
        /// for writers where writing might fail is not recommended, for that call finish() instead.
        fn drop(&mut self) {
            if self.inner.deflate_state.is_some() && !thread::panicking() {
                let _ = self.output_all();
            }
        }
    }

    #[cfg(test)]
    mod test {
        use super::*;
        use test_utils::{get_test_data, decompress_gzip};
        #[test]
        fn gzip_writer() {
            let data = get_test_data();
            let compressed = {
                let mut compressor = GzEncoder::new(Vec::with_capacity(data.len() / 3),
                                                    CompressionOptions::default());
                compressor.write_all(&data[0..data.len() / 2]).unwrap();
                compressor.write_all(&data[data.len() / 2..]).unwrap();
                compressor.finish().unwrap()
            };

            let res = decompress_gzip(&compressed);
            assert!(res == data);
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use test_utils::{get_test_data, decompress_to_end, decompress_zlib};
    use compression_options::CompressionOptions;
    use std::io::Write;

    #[test]
    fn deflate_writer() {
        let data = get_test_data();
        let compressed = {
            let mut compressor = DeflateEncoder::new(Vec::with_capacity(data.len() / 3),
                                                     CompressionOptions::high());
            // Write in multiple steps to see if this works as it's supposed to.
            compressor.write_all(&data[0..data.len() / 2]).unwrap();
            compressor.write_all(&data[data.len() / 2..]).unwrap();
            compressor.finish().unwrap()
        };

        let res = decompress_to_end(&compressed);
        assert!(res == data);
    }

    #[test]
    fn zlib_writer() {
        let data = get_test_data();
        let compressed = {
            let mut compressor = ZlibEncoder::new(Vec::with_capacity(data.len() / 3),
                                                  CompressionOptions::high());
            compressor.write_all(&data[0..data.len() / 2]).unwrap();
            compressor.write_all(&data[data.len() / 2..]).unwrap();
            compressor.finish().unwrap()
        };

        let res = decompress_zlib(&compressed);
        assert!(res == data);
    }

    #[test]
    /// Check if the the result of compressing after resetting is the same as before.
    fn writer_reset() {
        let data = get_test_data();
        let mut compressor = DeflateEncoder::new(Vec::with_capacity(data.len() / 3),
                                                 CompressionOptions::default());
        compressor.write_all(&data).unwrap();
        let res1 = compressor
            .reset(Vec::with_capacity(data.len() / 3))
            .unwrap();
        compressor.write_all(&data).unwrap();
        let res2 = compressor.finish().unwrap();
        assert!(res1 == res2);
    }

    #[test]
    fn writer_reset_zlib() {
        let data = get_test_data();
        let mut compressor = ZlibEncoder::new(Vec::with_capacity(data.len() / 3),
                                              CompressionOptions::default());
        compressor.write_all(&data).unwrap();
        let res1 = compressor
            .reset(Vec::with_capacity(data.len() / 3))
            .unwrap();
        compressor.write_all(&data).unwrap();
        let res2 = compressor.finish().unwrap();
        assert!(res1 == res2);
    }

    #[test]
    fn writer_sync() {
        let data = get_test_data();
        let compressed = {
            let mut compressor = DeflateEncoder::new(Vec::with_capacity(data.len() / 3),
                                                     CompressionOptions::default());
            let split = data.len() / 2;
            compressor.write_all(&data[..split]).unwrap();
            compressor.flush().unwrap();
            {
                let buf = &mut compressor.deflate_state.as_mut().unwrap().inner;
                let buf_len = buf.len();
                // Check for the sync marker. (excluding the header as it might not line
                // up with the byte boundary.)
                assert_eq!(buf[buf_len - 4..], [0, 0, 255, 255]);
            }
            compressor.write_all(&data[split..]).unwrap();
            compressor.finish().unwrap()
        };

        let decompressed = decompress_to_end(&compressed);

        assert!(decompressed == data);
    }

    #[test]
    fn writer_sync_multiple() {
        use std::cmp;
        let data = get_test_data();
        let compressed = {
            let mut compressor = DeflateEncoder::new(Vec::with_capacity(data.len() / 3),
                                                     CompressionOptions::default());
            let split = data.len() / 2;
            compressor.write_all(&data[..split]).unwrap();
            compressor.flush().unwrap();
            compressor.flush().unwrap();
            {
                let buf = &mut compressor.deflate_state.as_mut().unwrap().inner;
                let buf_len = buf.len();
                // Check for the sync marker. (excluding the header as it might not line
                // up with the byte boundary.)
                assert_eq!(buf[buf_len - 4..], [0, 0, 255, 255]);
            }
            compressor
                .write_all(&data[split..cmp::min(split + 2, data.len())])
                .unwrap();
            compressor.flush().unwrap();
            compressor
                .write_all(&data[cmp::min(split + 2, data.len())..])
                .unwrap();
            compressor.finish().unwrap()
        };

        let decompressed = decompress_to_end(&compressed);

        assert!(decompressed == data);

        let mut compressor = DeflateEncoder::new(Vec::with_capacity(data.len() / 3),
                                                 CompressionOptions::default());

        compressor.flush().unwrap();
        compressor.write_all(&[1, 2]).unwrap();
        compressor.flush().unwrap();
        compressor.write_all(&[3]).unwrap();
        compressor.flush().unwrap();
        let compressed = compressor.finish().unwrap();

        let decompressed = decompress_to_end(&compressed);

        assert_eq!(decompressed, [1, 2, 3]);
    }
}
