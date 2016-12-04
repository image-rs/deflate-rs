use std::io::Write;
use std::io;

use deflate_state::DeflateState;
use encoder_state::{EncoderState, BType};
use lzvalue::LZValue;
use lz77::{lz77_compress_block, LZ77Status};
use length_encode::huffman_lengths_from_frequency;
use huffman_lengths::{write_huffman_lengths, remove_trailing_zeroes, MIN_NUM_LITERALS_AND_LENGTHS,
                      MIN_NUM_DISTANCES};
use huffman_table::{MAX_CODE_LENGTH, FIXED_CODE_LENGTHS, FIXED_CODE_LENGTHS_DISTANCE};
use output_writer::OutputWriter;
use stored_block;
use stored_block::compress_block_stored;

#[derive(Eq, PartialEq, Debug, Copy, Clone)]
pub enum Flush {
    None,
    Sync,
    Partial,
    Block,
    Full,
    Finish,
}

pub fn flush_to_bitstream<W: Write>(buffer: &[LZValue],
                                    state: &mut EncoderState<W>)
                                    -> io::Result<()> {
    for &b in buffer {
        state.write_ldpair(b.value())?
    }
    state.write_end_of_block()
}

/// Determine if the block is long enough for it to be worth using dynamic huffman codes or just
/// Write the data directly.
fn block_type_for_length(length: usize) -> BType {
    // TODO: Do proper testing to determine what values make sense here
    if length < 5 {
        // For very short lengths, using fixed codes will be shorter as we don't need to
        // use two bytes to specify the length.
        BType::FixedHuffman
    } else if length < 20 {
        BType::NoCompression
    } else if length < 70 {
        BType::FixedHuffman
    } else {
        BType::DynamicHuffman
    }
}

#[cfg(test)]
/// Compress the input data using only fixed huffman codes.
///
/// Currently only used in tests.
pub fn compress_data_fixed(input: &[u8]) -> Vec<u8> {
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

/// Inner compression function used by both writers and simple compression functions.
pub fn compress_data_dynamic_n<W: Write>(input: &[u8],
                                         deflate_state: &mut DeflateState<W>,
                                         flush: Flush)
                                         -> io::Result<usize> {

    // If we are flushing and have not yet written anything to the output stream (which is the case
    // if is_first_window is true), we check if it will be shorter to used fixed huffman codes
    // or just a stored block instead of full compression.
    let block_type = if flush == Flush::Finish && deflate_state.lz77_state.is_first_window() {
        block_type_for_length(deflate_state.bytes_written + input.len())
    } else if flush == Flush::None || flush == Flush::Finish {
        BType::DynamicHuffman
    } else {
        println!("compress called with {:?}", flush);
        unimplemented!();
    };

    let mut bytes_written = 0;

    match block_type {
        BType::DynamicHuffman | BType::FixedHuffman => {
            match block_type {
                BType::DynamicHuffman => {
                    let mut slice = input;
                    while !deflate_state.lz77_state.is_last_block() {
                        let (written, status) =
                            lz77_compress_block(slice,
                                                &mut deflate_state.lz77_state,
                                                &mut deflate_state.input_buffer,
                                                &mut deflate_state.lz77_writer,
                                                flush == Flush::Finish);
                        // Bytes written in this call
                        bytes_written += written;
                        // Total bytes written since the compression process started
                        deflate_state.bytes_written += bytes_written;

                        if status == LZ77Status::NeedInput {
                            return Ok(bytes_written);
                        }

                        // Increment start of input data
                        slice = &slice[written..];
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

                    let (written, _) = lz77_compress_block(input,
                                                           &mut deflate_state.lz77_state,
                                                           &mut deflate_state.input_buffer,
                                                           &mut deflate_state.lz77_writer,
                                                           true);
                    bytes_written += written;
                    deflate_state.bytes_written += written;
                    deflate_state.encoder_state
                        .update_huffman_table(&FIXED_CODE_LENGTHS, &FIXED_CODE_LENGTHS_DISTANCE)
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

            assert!(flush != Flush::None);

            deflate_state.encoder_state
                .writer
                .write_bits(stored_block::STORED_FIRST_BYTE_FINAL.into(), 3)
                .unwrap();
            deflate_state.encoder_state.flush().unwrap();
            let rem = deflate_state.input_buffer.add_data(input);
            // There shouldn't be any leftover data here.
            assert!(rem.is_none());
            // Write the pending bytes
            let _ = compress_block_stored(deflate_state.input_buffer.get_buffer(),
                                          &mut deflate_state.encoder_state.writer)?;

            // Keep track of how many extra bytes we consumed in this call.
            let written = input.len();
            bytes_written += written;
            deflate_state.bytes_written += written;
        }
    }

    deflate_state.encoder_state.flush().map(|()| bytes_written)
}

#[cfg(test)]
mod test {
    use super::*;
    use test_utils::{get_test_data, decompress_to_end};

    #[test]
    /// Test compressing a short string using fixed encoding.
    fn fixed_string_mem() {
        use std::str;

        let test_data = String::from("                    GNU GENERAL PUBLIC LICENSE").into_bytes();
        let compressed = compress_data_fixed(&test_data);

        let result = decompress_to_end(&compressed);
        println!("Output: `{}`", str::from_utf8(&result).unwrap());
        assert_eq!(test_data, result);
    }

    #[test]
    fn fixed_data() {
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
    fn fixed_example() {
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
    /// Test compression from a file.
    fn fixed_string_file() {
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
        // Not using assert_eq here deliberately to avoid massive amounts of output spam.
        assert!(input == result);
    }
}
