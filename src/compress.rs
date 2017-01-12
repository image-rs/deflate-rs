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
use stored_block::{compress_block_stored, write_stored_header};

#[derive(Eq, PartialEq, Debug, Copy, Clone)]
pub enum Flush {
    None,
    Sync,
    _Partial,
    _Block,
    _Full,
    Finish,
}

/// Write all the lz77 encoded data in the buffer using the specified `EncoderState`, and finish
/// with the end of block code.
///
/// Returns `Err` if writing should fail at any point.
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
    if length < 25 {
        // For very short lengths, using fixed codes will be shorter as we don't need to
        // use two bytes to specify the length.
        BType::FixedHuffman
        // } else if length < 20 {
        // BType::NoCompression
        // } else if length < 70 {
        // BType::FixedHuffman
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

fn write_stored_block<W: Write>(input: &[u8],
                                deflate_state: &mut DeflateState<W>,
                                final_block: bool)
                                -> io::Result<usize> {
    // Write the block header
    write_stored_header(&mut deflate_state.encoder_state.writer, final_block)?;
    // Output some extra zeroes if needed to align with the byte boundary.
    deflate_state.encoder_state.flush()?;

    if input.len() > 0 {
        // Add the current input data to the input buffer.
        let rem = deflate_state.input_buffer.add_data(input);

        if rem.is_some() {
            return Err(io::Error::new(io::ErrorKind::InvalidInput,
                                      "write_stored_block called with more data\
                                       than currently supported!"));
        }

        // Write the length of the data and the actual (uncompressed) data.
        compress_block_stored(deflate_state.input_buffer.get_buffer(),
                              &mut deflate_state.encoder_state.writer)
    } else {
        // Make sure we output an empty block if the input is empty.
        compress_block_stored(input, &mut deflate_state.encoder_state.writer)
    }
}

/// Inner compression function used by both the writers and the simple compression functions.
pub fn compress_data_dynamic_n<W: Write>(input: &[u8],
                                         deflate_state: &mut DeflateState<W>,
                                         flush: Flush)
                                         -> io::Result<usize> {

    // If we are flushing and have not yet written anything to the output stream (which is the case
    // if is_first_window is true), we check if it will be shorter to used fixed huffman codes
    // or just a stored block instead of full compression.
    let block_type = if (flush == Flush::Finish || flush == Flush::Sync) &&
                        deflate_state.lz77_state.is_first_window() {
        block_type_for_length(input.len().saturating_add(deflate_state.bytes_written as usize))
    } else if flush == Flush::None || flush == Flush::Finish ||
                               flush == Flush::Sync {
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
                    loop {
                        let (written, status) =
                            lz77_compress_block(slice,
                                                &mut deflate_state.lz77_state,
                                                &mut deflate_state.input_buffer,
                                                &mut deflate_state.lz77_writer,
                                                flush);
                        // Bytes written in this call
                        bytes_written += written;
                        // Total bytes written since the compression process started
                        deflate_state.bytes_written += bytes_written as u64;

                        if status == LZ77Status::NeedInput {
                            // If we've consumed all the data input so far, and we're not
                            // finishing or syncing or ending the block here, simply return
                            // the number of bytes consumed so far.
                            return Ok(bytes_written);
                        }

                        // Increment start of input data
                        slice = &slice[written..];
                        deflate_state.encoder_state
                            .write_start_of_block(false, deflate_state.lz77_state.is_last_block())?;

                        // Generate the lengths of the huffman codes we will be using, using the
                        // frequency of the different symbols/lengths/distances.
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
                        // Output the lengths of the huffman codes used in this block.
                        write_huffman_lengths(&l_lengths,
                                              &d_lengths,
                                              &mut deflate_state.encoder_state.writer)?;

                        // Output update the huffman table that will be used to encode the
                        // lz77-compressed data.
                        deflate_state.encoder_state
                            .update_huffman_table(&l_lengths, &d_lengths)?;

                        // write the huffman compressed data and end of block code.
                        flush_to_bitstream(deflate_state.lz77_writer.get_buffer(),
                                           &mut deflate_state.encoder_state)?;

                        // End of block is written in flush_to_bitstream.

                        // Clear the current lz77 data in the writer for the next call.
                        deflate_state.lz77_writer.clear();
                        if status == LZ77Status::Finished {
                            break;
                        }
                    }
                }
                BType::FixedHuffman => {
                    // Lz77-compress the block. Since this block of code will be ran only at a
                    // sync or finish point, and any previous calls will ensure that there will
                    // only be at most one window size of data left, the function is called only
                    // once here.
                    let (written, _) = lz77_compress_block(input,
                                                           &mut deflate_state.lz77_state,
                                                           &mut deflate_state.input_buffer,
                                                           &mut deflate_state.lz77_writer,
                                                           flush);

                    bytes_written += written;
                    deflate_state.bytes_written += written as u64;
                    // Update the state to use the fixed(pre-defined) huffman codes.
                    deflate_state.encoder_state
                        .update_huffman_table(&FIXED_CODE_LENGTHS, &FIXED_CODE_LENGTHS_DISTANCE)?;
                    deflate_state.encoder_state.write_start_of_block(true, flush == Flush::Finish)?;
                    flush_to_bitstream(deflate_state.lz77_writer.get_buffer(),
                                       &mut deflate_state.encoder_state)?;
                    // Clear the current lz77 data in the writer for the next call.
                    deflate_state.lz77_writer.clear();
                }
                BType::NoCompression => {
                    unreachable!();
                }
            }

        }
        BType::NoCompression => {
            assert!(flush != Flush::None);

            write_stored_block(input, deflate_state, flush == Flush::Finish)?;

            // Keep track of how many extra bytes we consumed in this call.
            let written = input.len();
            bytes_written += written;
            deflate_state.bytes_written += written as u64;
        }
    }

    if flush == Flush::Sync {
        write_stored_block(&[], deflate_state, false)?;
    }

    // Make sure we've output everything, and return the number of bytes written if everything
    // went well.
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

        assert_eq!(input.len(), result.len());
        // Not using assert_eq here deliberately to avoid massive amounts of output spam.
        assert!(input == result);
    }
}
