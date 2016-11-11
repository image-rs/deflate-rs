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

use std::io::Write;
use std::io;

use byteorder::BigEndian;

use huffman_table::*;
use input_buffer::InputBuffer;
use huffman_lengths::{write_huffman_lengths, remove_trailing_zeroes, MIN_NUM_LITERALS_AND_LENGTHS,
                      MIN_NUM_DISTANCES};
use length_encode::huffman_lengths_from_frequency;
use checksum::RollingChecksum;
use encoder_state::{EncoderState, BType};
use stored_block::compress_block_stored;
use output_writer::{OutputWriter, DynamicWriter};
use deflate_state::DeflateState;

#[doc(hidden)]
pub use lz77::lz77_compress;

pub use compression_options::CompressionOptions;

/// Determine if the block is long enough for it to be worth using dynamic huffman codes or just
/// Write the data directly
fn block_type_for_length(length: usize) -> BType {
    // TODO: Do proper testing to determine what values make sense here
    if length < 20 {
        BType::NoCompression
    } else if length < 70 {
        BType::FixedHuffman
    } else {
        BType::DynamicHuffman
    }
}

fn flush_to_bitstream<W: std::io::Write>(buffer: &[lzvalue::LZValue],
                                         state: &mut EncoderState<W>)
                                         -> io::Result<()> {
    for &b in buffer {
        state.write_ldpair(b.value())?
    }
    state.write_end_of_block()?;
    Ok(())
}

#[cfg(test)]
fn compress_data_fixed(input: &[u8]) -> Vec<u8> {
    use lz77::lz77_compress;

    let mut writer = Vec::new();
    {
        let mut state = EncoderState::fixed(&mut writer);
        let compressed = lz77_compress(input).unwrap();

        // We currently don't split blocks here(this function is just used for tests anyhow)
        state.write_start_of_block(true, true).expect("Write error!");
        flush_to_bitstream(&compressed, &mut state).expect("Write error!");

        state.flush().expect("Write error!");

    }
    writer
}

fn compress_data_dynamic<RC: RollingChecksum, W: Write>(input: &[u8],
                                                        mut writer: &mut W,
                                                        mut checksum: &mut RC,
                                                        compression_options: CompressionOptions)
                                                        -> io::Result<()> {
    checksum.update_from_slice(input);

    let mut deflate_state = DeflateState {
        input_buffer: InputBuffer::empty(),
        lz77_state: lz77::LZ77State::new(input, compression_options.max_hash_checks),
        encoder_state: EncoderState::new(huffman_table::HuffmanTable::empty(), &mut writer),
        lz77_writer: DynamicWriter::new(),
        compression_options: compression_options, //                writer: writer,
    };

    let block_type = block_type_for_length(input.len());

    match block_type {
        BType::DynamicHuffman | BType::FixedHuffman => {
            match block_type {
                BType::DynamicHuffman => {
                    let mut slice = input;
                    while !deflate_state.lz77_state.is_last_block() {
                        let (bytes_written, _) =
                            lz77::lz77_compress_block(&slice,
                                                      &mut deflate_state.lz77_state,
                                                      &mut deflate_state.input_buffer,
                                                      &mut deflate_state.lz77_writer,
                                                      true);
                        // Increment start of input data
                        slice = &slice[bytes_written..];
                        deflate_state.encoder_state
                            .write_start_of_block(false, deflate_state.lz77_state.is_last_block())?;

                        let (l_lengths, d_lengths) = {
                            let (l_freqs, d_freqs) = deflate_state.lz77_writer.get_frequencies();
                            // The huffman spec allows us to exclude zeroes at the end of the table
                            // of huffman lengths. Since a frequency of 0 will give an huffman
                            // length of 0. We strip off the trailing zeroes before even generating
                            // the lengths to save some work.
                            // There is however a minimum number of values we have to keep according
                            // to the deflate spec.
                            (
                                huffman_lengths_from_frequency(
                                    remove_trailing_zeroes(l_freqs, MIN_NUM_LITERALS_AND_LENGTHS),
                                    MAX_CODE_LENGTH
                            ),
                                huffman_lengths_from_frequency(
                                    remove_trailing_zeroes(d_freqs, MIN_NUM_DISTANCES),
                                    MAX_CODE_LENGTH)
                            )
                        };
                        write_huffman_lengths(&l_lengths,
                                              &d_lengths,
                                              &mut deflate_state.encoder_state.writer)?;

                        deflate_state.encoder_state
                            .update_huffman_table(&l_lengths, &d_lengths)
                            .expect("Fatal error!: Failed to create huffman table!");

                        flush_to_bitstream(deflate_state.lz77_writer.get_buffer(),
                                           &mut deflate_state.encoder_state)?;

                        // End of block is written in flush_to_bitstream.
                        deflate_state.lz77_writer.clear();
                    }
                }
                BType::FixedHuffman => {

                    lz77::lz77_compress_block(input,
                                              &mut deflate_state.lz77_state,
                                              &mut deflate_state.input_buffer,
                                              &mut deflate_state.lz77_writer,
                                              true);
                    deflate_state.encoder_state
                        .update_huffman_table(&huffman_table::FIXED_CODE_LENGTHS,
                                              &huffman_table::FIXED_CODE_LENGTHS_DISTANCE)
                        .unwrap();
                    deflate_state.encoder_state.write_start_of_block(true, true)?;
                    flush_to_bitstream(deflate_state.lz77_writer.get_buffer(),
                                       &mut deflate_state.encoder_state)?;
                    deflate_state.lz77_writer.clear();
                }
                BType::NoCompression => {
                    unreachable!();
                }
            }

        }
        BType::NoCompression => {
            use bitstream::BitWriter;

            deflate_state.encoder_state
                .writer
                .write_bits(stored_block::STORED_FIRST_BYTE_FINAL.into(), 3)
                .unwrap();
            deflate_state.encoder_state.flush().unwrap();
            compress_block_stored(input, &mut deflate_state.encoder_state.writer)?;
        }
    }

    deflate_state.encoder_state.flush()
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
    compress_data_dynamic(input,
                          &mut writer,
                          &mut checksum::NoChecksum::new(),
                          options)
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
    use stored_block::compress_data_stored;
    use super::compress_data_fixed;

    /// Helper function to decompress into a `Vec<u8>`
    fn decompress_to_end(input: &[u8]) -> Vec<u8> {
        // use std::str;
        // let mut inflater = super::inflate::InflateStream::new();
        // let mut out = Vec::new();
        // let mut n = 0;
        // println!("input len {}", input.len());
        // while n < input.len() {
        // let res = inflater.update(&input[n..]) ;
        // if let Ok((num_bytes_read, result)) = res {
        // println!("result len {}, bytes_read {}", result.len(), num_bytes_read);
        // n += num_bytes_read;
        // out.extend(result);
        // } else {
        // println!("Output: `{}`", str::from_utf8(&out).unwrap());
        // println!("Output decompressed: {}", out.len());
        // res.unwrap();
        // }
        //
        // }
        // out

        use std::io::Read;
        use flate2::read::DeflateDecoder;

        let mut result = Vec::new();
        let i = &input[..];
        let mut e = DeflateDecoder::new(i);

        let res = e.read_to_end(&mut result);
        if let Ok(n) = res {
            println!("{} bytes read successfully", n);
        } else {
            println!("result size: {}", result.len());
            res.unwrap();
        }
        result
    }

    use super::*;

    fn get_test_file_data(name: &str) -> Vec<u8> {
        use std::fs::File;
        use std::io::Read;
        let mut input = Vec::new();
        let mut f = File::open(name).unwrap();

        f.read_to_end(&mut input).unwrap();
        input
    }

    fn get_test_data() -> Vec<u8> {
        use std::env;
        let path = env::var("TEST_FILE").unwrap_or("tests/pg11.txt".to_string());
        get_test_file_data(&path)
    }

    #[test]
    fn test_no_compression_one_chunk() {
        let test_data = vec![1u8, 2, 3, 4, 5, 6, 7, 8];
        let compressed = compress_data_stored(&test_data);
        let result = decompress_to_end(&compressed);
        assert_eq!(test_data, result);
    }

    #[test]
    fn test_no_compression_multiple_chunks() {
        let test_data = vec![32u8; 40000];
        let compressed = compress_data_stored(&test_data);
        let result = decompress_to_end(&compressed);
        assert_eq!(test_data, result);
    }

    #[test]
    fn test_no_compression_string() {
        let test_data = String::from("This is some text, this is some more text, this is even \
                                      more text, lots of text here.")
            .into_bytes();
        let compressed = compress_data_stored(&test_data);
        let result = decompress_to_end(&compressed);
        assert_eq!(test_data, result);
    }

    #[test]
    fn test_fixed_string_mem() {
        use std::str;

        let test_data = String::from("                    GNU GENERAL PUBLIC LICENSE").into_bytes();
        let compressed = compress_data_fixed(&test_data);

        let result = decompress_to_end(&compressed);
        println!("Output: `{}`", str::from_utf8(&result).unwrap());
        assert_eq!(test_data, result);
    }

    #[test]
    fn test_fixed_data() {
        let data = vec![190u8; 400];
        let compressed = compress_data_fixed(&data);
        let result = decompress_to_end(&compressed);

        println!("data len: {}, result len: {}", data.len(), result.len());
        for n in compressed {
            println!("{:#b}", n)
        }
        assert_eq!(data, result);
    }

    /// Test deflate example.
    ///
    /// Check if the encoder produces the same code as the example given by Mark Adler here:
    /// https://stackoverflow.com/questions/17398931/deflate-encoding-with-static-huffman-codes/17415203
    #[test]
    fn test_fixed_example() {
        let test_data = b"Deflate late";
        // let check =
        // [0x73, 0x49, 0x4d, 0xcb, 0x49, 0x2c, 0x49, 0x55, 0xc8, 0x49, 0x2c, 0x49, 0x5, 0x0];
        let check = [0x73, 0x49, 0x4d, 0xcb, 0x49, 0x2c, 0x49, 0x55, 0x00, 0x11, 0x00];
        let compressed = compress_data_fixed(test_data);
        assert_eq!(&compressed, &check);
        let decompressed = decompress_to_end(&compressed);
        assert_eq!(&decompressed, test_data)
    }

    #[test]
    fn test_fixed_string_file() {
        use std::str;

        let input = get_test_data();

        let compressed = compress_data_fixed(&input);
        println!("Compressed len: {}", compressed.len());
        let result = decompress_to_end(&compressed);
        // let out1 = str::from_utf8(&input).unwrap();
        // let out2 = str::from_utf8(&result).unwrap();
        // println!("Orig:\n{}", out1);
        // println!("Compr:\n{}", out2);
        assert_eq!(input.len(), result.len());
        // Not using assert_eq here deliberately to avoid massive amounts of output spam
        assert!(input == result);
    }



    #[test]
    fn test_dynamic_string_mem() {
        use std::str;
        let test_data = String::from("                    GNU GENERAL PUBLIC LICENSE").into_bytes();
        let compressed = deflate_bytes(&test_data);

        assert!(compressed.len() < test_data.len());

        let result = decompress_to_end(&compressed);
        assert_eq!(test_data, result);
    }

    #[test]
    fn test_dynamic_string_file() {
        use std::str;
        let input = get_test_data();
        let compressed = deflate_bytes(&input);

        println!("Compressed len: {}", compressed.len());

        let result = decompress_to_end(&compressed);
        // Check that we actually managed to compress the input
        assert!(compressed.len() < input.len());
        // Not using assert_eq here deliberately to avoid massive amounts of output spam
        assert!(input == result);
    }

    fn decompress_zlib(compressed: &[u8]) -> Vec<u8> {
        use std::io::Read;
        use flate2::read::ZlibDecoder;
        let mut e = ZlibDecoder::new(&compressed[..]);

        let mut result = Vec::new();
        e.read_to_end(&mut result).unwrap();
        result
    }

    #[test]
    fn test_file_zlib() {
        let test_data = get_test_data();

        let compressed = deflate_bytes_zlib(&test_data);
        // {
        // use std::fs::File;
        // use std::io::Write;
        // let mut f = File::create("out.zlib").unwrap();
        // f.write_all(&compressed).unwrap();
        // }

        println!("compressed length: {}", compressed.len());

        assert!(compressed.len() < test_data.len());

        let result = decompress_zlib(&compressed);

        assert!(&test_data == &result);
    }

    #[test]
    fn test_zlib_short() {
        let test_data = [10, 20, 30, 40, 55];
        let compressed = deflate_bytes_zlib(&test_data);



        let result = decompress_zlib(&compressed);
        assert_eq!(&test_data, result.as_slice());
    }

    #[test]
    fn test_zlib_last_block() {
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
}
