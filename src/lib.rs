//! An implementation an encoder using [DEFLATE](http://www.gzip.org/zlib/rfc-deflate.html)
//! compression algorightm in pure rust.
//!
//! This library provides functions to compress data (currently only in-memory) using DEFLATE,
//! both with and without a [zlib](https://tools.ietf.org/html/rfc1950) header/trailer
//! The current implementation is still a bit lacking speed-wise compared to C-libraries like zlib and miniz.

#[cfg(test)]
extern crate flate2;
// #[cfg(test)]
// extern crate inflate;

extern crate adler32;
extern crate byteorder;

mod compression_options;
mod huffman_table;
mod lz77;
mod lzvalue;
mod chained_hash_table;
mod length_encode;
mod output_writer;
mod stored_block;
mod huffman_lengths;
mod zlib;
mod checksum;
mod bit_reverse;
mod bitstream;
mod encoder_state;
mod matching;
mod input_buffer;
mod deflate_state;
mod compress;
#[cfg(test)]
mod test_utils;

use std::io::Write;
use std::io;

use byteorder::BigEndian;

use checksum::RollingChecksum;
use deflate_state::DeflateState;
use compress::compress_data_dynamic_n;

#[doc(hidden)]
pub use lz77::lz77_compress;

pub use compression_options::{CompressionOptions, SpecialOptions};
pub use compress::{DeflateEncoder, ZlibEncoder};

fn compress_data_dynamic<RC: RollingChecksum, W: Write>(input: &[u8],
                                                        writer: &mut W,
                                                        mut checksum: RC,
                                                        compression_options: CompressionOptions)
                                                        -> io::Result<usize> {
    checksum.update_from_slice(input);
    let mut deflate_state = DeflateState::new(compression_options, writer);
    compress_data_dynamic_n(input, &mut deflate_state, true)
}

/// Compress the given slice of bytes with DEFLATE compression.
///
/// Returns a Vec<u8> of the compressed data.
///
/// # Examples
///
/// ```
/// use deflate::{deflate_bytes_conf, CompressionOptions};
/// let data = b"This is some test data";
/// let options = CompressionOptions::default();
/// let compressed_data = deflate_bytes_conf(data, options);
/// # let _ = compressed_data;
/// ```
pub fn deflate_bytes_conf(input: &[u8], options: CompressionOptions) -> Vec<u8> {
    let mut writer = Vec::with_capacity(input.len() / 3);
    compress_data_dynamic(input, &mut writer, checksum::NoChecksum::new(), options)
        .expect("Write error!");
    writer
}

/// Compress the given slice of bytes with DEFLATE compression using the default compression
/// level.
///
/// Returns a Vec<u8> of the compressed data.
///
/// # Examples
///
/// ```
/// use deflate::deflate_bytes;
/// let data = b"This is some test data";
/// let compressed_data = deflate_bytes(data);
/// # let _ = compressed_data;
/// ```
pub fn deflate_bytes(input: &[u8]) -> Vec<u8> {
    deflate_bytes_conf(input, CompressionOptions::default())
}

/// Compress the given slice of bytes with DEFLATE compression, including a zlib header and trailer.
///
/// Returns a Vec<u8> of the compressed data.
///
/// Zlib dictionaries are not yet suppored.
///
/// # Examples
///
/// ```
/// use deflate::{deflate_bytes_zlib_conf, CompressionOptions};
/// let options = CompressionOptions::default();
/// let data = b"This is some test data";
/// let compressed_data = deflate_bytes_zlib_conf(data, options);
/// # let _ = compressed_data;
/// ```
pub fn deflate_bytes_zlib_conf(input: &[u8], options: CompressionOptions) -> Vec<u8> {
    use byteorder::WriteBytesExt;
    let mut writer = Vec::with_capacity(input.len() / 3);
    // Write header
    zlib::write_zlib_header(&mut writer, zlib::CompressionLevel::Default)
        .expect("Write error when writing zlib header!");

    let mut checksum = checksum::Adler32Checksum::new();
    compress_data_dynamic(input, &mut writer, &mut checksum, options)
        .expect("Write error when writing compressed data!");

    let hash = checksum.current_hash();

    writer.write_u32::<BigEndian>(hash).expect("Write error when writing checksum!");
    writer
}

/// Compress the given slice of bytes with DEFLATE compression, including a zlib header and trailer,
/// using the default compression level.
///
/// Returns a Vec<u8> of the compressed data.
///
/// Zlib dictionaries are not yet suppored.
///
/// # Examples
///
/// ```
/// use deflate::deflate_bytes_zlib;
/// let data = b"This is some test data";
/// let compressed_data = deflate_bytes_zlib(data);
/// # let _ = compressed_data;
/// ```
pub fn deflate_bytes_zlib(input: &[u8]) -> Vec<u8> {
    deflate_bytes_zlib_conf(input, CompressionOptions::default())
}

#[cfg(test)]
mod test {
    use super::*;
    use std::io::Write;

    use test_utils::{get_test_data, decompress_to_end, decompress_zlib};

    /// Write data to the writer in chunks of chunk_size.
    fn chunked_write<W: Write>(mut writer: W, data: &[u8], chunk_size: usize) {
        for chunk in data.chunks(chunk_size) {
            let bytes_written = writer.write(&chunk).unwrap();
            assert_eq!(bytes_written, chunk.len());
        }
        writer.flush().unwrap();
    }

    #[test]
    fn dynamic_string_mem() {
        use std::str;
        let test_data = String::from("                    GNU GENERAL PUBLIC LICENSE").into_bytes();
        let compressed = deflate_bytes(&test_data);

        assert!(compressed.len() < test_data.len());

        let result = decompress_to_end(&compressed);
        assert_eq!(test_data, result);
    }

    #[test]
    fn dynamic_string_file() {
        use std::str;
        let input = get_test_data();
        let compressed = deflate_bytes(&input);

        println!("Compressed len: {}", compressed.len());

        let result = decompress_to_end(&compressed);
        // Not using assert_eq here deliberately to avoid massive amounts of output spam
        assert!(input == result);
        // Check that we actually managed to compress the input
        assert!(compressed.len() < input.len());
    }

    #[test]
    fn file_zlib() {
        let test_data = get_test_data();

        let compressed = deflate_bytes_zlib(&test_data);
        // {
        //     use std::fs::File;
        //     use std::io::Write;
        //     let mut f = File::create("out.zlib").unwrap();
        //     f.write_all(&compressed).unwrap();
        // }

        println!("compressed length: {}", compressed.len());

        let result = decompress_zlib(&compressed);

        assert!(&test_data == &result);
        assert!(compressed.len() < test_data.len());
    }

    #[test]
    fn zlib_short() {
        let test_data = [10, 20, 30, 40, 55];
        let compressed = deflate_bytes_zlib(&test_data);



        let result = decompress_zlib(&compressed);
        assert_eq!(&test_data, result.as_slice());
    }

    #[test]
    fn zlib_last_block() {
        let mut test_data = vec![22; 32768];
        test_data.extend(&[5, 2, 55, 11, 12]);
        let compressed = deflate_bytes_zlib(&test_data);
        // {
        // use std::fs::File;
        // use std::io::Write;
        // let mut f = File::create("out_block.zlib").unwrap();
        // f.write_all(&compressed).unwrap();
        // }

        let result = decompress_zlib(&compressed);
        assert!(test_data == result);
    }

    fn chunk_test(chunk_size: usize) {
        let mut compressed = Vec::with_capacity(32000);
        let data = get_test_data();
        {
            let mut compressor = ZlibEncoder::new(&mut compressed, CompressionOptions::high());
            chunked_write(&mut compressor, &data, chunk_size);
        }
        let compressed2 = deflate_bytes_zlib_conf(&data, CompressionOptions::high());
        let res = decompress_zlib(&compressed);
        assert!(res == data);
        assert_eq!(compressed.len(), compressed2.len());
        assert!(compressed == compressed2);
    }

    #[test]
    /// Test the writer by inputing data in one chunk at the time.
    fn zlib_writer_chunks() {
        use input_buffer::BUFFER_SIZE;
        chunk_test(1);
        chunk_test(50);
        chunk_test(400);
        chunk_test(32768);
        chunk_test(BUFFER_SIZE);
        chunk_test(50000);
        chunk_test((32768 * 2) + 258);
    }
}
