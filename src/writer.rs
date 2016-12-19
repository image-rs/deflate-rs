use std::io::Write;
use std::io;

use byteorder::{WriteBytesExt, BigEndian};

use checksum::{Adler32Checksum, RollingChecksum};
use compress::compress_data_dynamic_n;
use compress::Flush;
use deflate_state::DeflateState;
use compression_options::CompressionOptions;
use zlib::{write_zlib_header, CompressionLevel};
use std::thread;

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
        self.output_all().map(|_| ())?;
        // We have to move the inner state out of the encoder, and replace it with `None`
        // to let the `DeflateEncoder` drop safely.
        let state = self.deflate_state.take();
        Ok(state.unwrap().encoder_state.writer.w)
    }

    /// Resets the encoder (except the compression options), replacing the current writer
    /// with a new one, returning the old one.
    pub fn reset(&mut self, w: W) -> io::Result<W> {
        self.output_all().map(|_| ())?;
        self.deflate_state.as_mut().unwrap().reset(w)
    }

    /// Output all pending data as if encoding is done, but without resetting anything
    fn output_all(&mut self) -> io::Result<usize> {
        compress_data_dynamic_n(&[],
                                &mut self.deflate_state.as_mut().unwrap(),
                                Flush::Finish)
    }
}

impl<W: Write> io::Write for DeflateEncoder<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        compress_data_dynamic_n(buf, &mut self.deflate_state.as_mut().unwrap(), Flush::None)
    }

    fn flush(&mut self) -> io::Result<()> {
        compress_data_dynamic_n(&[], &mut self.deflate_state.as_mut().unwrap(), Flush::Sync)
            .map(|_| ())
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
    fn output_all(&mut self) -> io::Result<usize> {
        self.check_write_header()?;
        let n = compress_data_dynamic_n(&[],
                                        &mut self.deflate_state.as_mut().unwrap(),
                                        Flush::Finish)?;
        self.write_trailer()?;
        Ok(n)
    }

    /// Encode all pending data to the contained writer, consume this `ZlibEncoder`,
    /// and return the contained writer if writing succeeds.
    pub fn finish(mut self) -> io::Result<W> {
        self.output_all()?;
        // We have to move the inner state out of the encoder, and replace it with `None`
        // to let the `DeflateEncoder` drop safely.
        let inner = self.deflate_state.take();
        Ok(inner.unwrap().encoder_state.writer.w)
    }

    /// Resets the encoder (except the compression options), replacing the current writer
    /// with a new one, returning the old one.
    pub fn reset(&mut self, writer: W) -> io::Result<W> {
        self.check_write_header()?;
        compress_data_dynamic_n(&[],
                                &mut self.deflate_state.as_mut().unwrap(),
                                Flush::Finish)?;
        self.write_trailer()?;
        self.header_written = false;
        self.checksum = Adler32Checksum::new();
        self.deflate_state.as_mut().unwrap().reset(writer)
    }

    /// Check if a zlib header should be written.
    fn check_write_header(&mut self) -> io::Result<()> {
        if !self.header_written {
            write_zlib_header(&mut self.deflate_state.as_mut().unwrap().encoder_state.writer,
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
            .encoder_state
            .writer
            .write_u32::<BigEndian>(hash)
    }
}

impl<W: Write> io::Write for ZlibEncoder<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.check_write_header()?;
        self.checksum.update_from_slice(buf);
        compress_data_dynamic_n(buf, &mut self.deflate_state.as_mut().unwrap(), Flush::None)
    }

    /// Flush the encoder.
    ///
    /// This will flush the encoder, emulating the Sync flush method from Zlib.
    /// This essentially finishes the current block, and sends an additional empty stored block to
    /// the writer.
    fn flush(&mut self) -> io::Result<()> {
        compress_data_dynamic_n(&[], &mut self.deflate_state.as_mut().unwrap(), Flush::Sync)
            .map(|_| ())
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
            compressor.write(&data[0..data.len() / 2]).unwrap();
            compressor.write(&data[data.len() / 2..]).unwrap();
            compressor.finish().unwrap()
        };
        println!("writer compressed len:{}", compressed.len());
        let res = decompress_to_end(&compressed);
        assert!(res == data);
    }

    #[test]
    fn zlib_writer() {
        let data = get_test_data();
        let compressed = {
            let mut compressor = ZlibEncoder::new(Vec::with_capacity(data.len() / 3),
                                                  CompressionOptions::high());
            compressor.write(&data[0..data.len() / 2]).unwrap();
            compressor.write(&data[data.len() / 2..]).unwrap();
            compressor.finish().unwrap()
        };
        println!("writer compressed len:{}", compressed.len());
        let res = decompress_zlib(&compressed);
        assert!(res == data);
    }



    #[test]
    /// Check if the the result of compressing after resetting is the same as before.
    fn writer_reset() {
        let data = get_test_data();
        let mut compressor = DeflateEncoder::new(Vec::with_capacity(data.len() / 3),
                                                 CompressionOptions::default());
        compressor.write(&data).unwrap();
        let res1 = compressor.reset(Vec::with_capacity(data.len() / 3)).unwrap();
        compressor.write(&data).unwrap();
        let res2 = compressor.finish().unwrap();
        assert!(res1 == res2);
    }

    #[test]
    fn writer_reset_zlib() {
        let data = get_test_data();
        let mut compressor = ZlibEncoder::new(Vec::with_capacity(data.len() / 3),
                                              CompressionOptions::default());
        compressor.write(&data).unwrap();
        let res1 = compressor.reset(Vec::with_capacity(data.len() / 3)).unwrap();
        compressor.write(&data).unwrap();
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
            compressor.write(&data[..split]).unwrap();
            compressor.flush().unwrap();
            compressor.write(&data[split..]).unwrap();
            compressor.finish().unwrap()
        };


        let res = decompress_to_end(&compressed);
        assert!(res == data);
    }
}
