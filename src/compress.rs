use std::io::Write;
use std::io;

use deflate_state::DeflateState;
use encoder_state::EncoderState;
use lzvalue::LZValue;
use lz77::{lz77_compress_block, LZ77Status, decompress_lz77_with_backbuffer};
use huffman_lengths::{gen_huffman_lengths, write_huffman_lengths, BlockType};
use output_writer::OutputWriter;
use stored_block::{compress_block_stored, write_stored_header, MAX_STORED_BLOCK_LENGTH};

/// Flush mode to use when compressing input received in multiple steps.
///
/// (The more obscure ZLIB flush modes are not implemented.)
#[derive(Eq, PartialEq, Debug, Copy, Clone)]
pub enum Flush {
    // Simply wait for more input when we are out of input data to process.
    None,
    // Send a "sync block", corresponding to Z_SYNC_FLUSH in zlib. This finishes compressing and
    // outputting all pending data, and then outputs an empty stored block.
    // (That is, the block header indicating a stored block followed by `0000FFFF`).
    Sync,
    _Partial,
    _Block,
    _Full,
    // Finish compressing and output all remaining input.
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
        state.write_lzvalue(b.value())?
    }
    state.write_end_of_block()
}

/// Compress the input data using only fixed huffman codes.
///
/// Currently only used in tests.
#[cfg(test)]
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

    // If the input is not zero, we write stored blocks for the input data.
    if !input.is_empty() {
        let mut written = 0;
        let mut i = input.chunks(MAX_STORED_BLOCK_LENGTH).peekable();

        while let Some(chunk) = i.next() {
            let last_chunk = i.peek().is_none();
            // Write the block header
            write_stored_header(&mut deflate_state.encoder_state.writer,
                                final_block && last_chunk)?;

            // Write the actual data.
            written += compress_block_stored(chunk, &mut deflate_state.encoder_state.writer)?;

        }
        Ok(written)
    } else {
        // If the input length is zero, we output an empty block. This is used for syncing.
        write_stored_header(&mut deflate_state.encoder_state.writer, final_block)?;
        compress_block_stored(&[], &mut deflate_state.encoder_state.writer)
    }
}

/// Inner compression function used by both the writers and the simple compression functions.
pub fn compress_data_dynamic_n<W: Write>(input: &[u8],
                                         deflate_state: &mut DeflateState<W>,
                                         flush: Flush)
                                         -> io::Result<usize> {
    let mut bytes_written = 0;

    let mut slice = input;
    loop {
        let (written, status, position) = lz77_compress_block(slice,
                                                              &mut deflate_state.lz77_state,
                                                              &mut deflate_state.input_buffer,
                                                              &mut deflate_state.lz77_writer,
                                                              flush);

        // Bytes written in this call
        bytes_written += written;
        // Total bytes written since the compression process started
        // TODO: Should we realistically have to worry about overflowing here?
        deflate_state.bytes_written += bytes_written as u64;

        if status == LZ77Status::NeedInput {
            // If we've consumed all the data input so far, and we're not
            // finishing or syncing or ending the block here, simply return
            // the number of bytes consumed so far.
            return Ok(bytes_written);
        }

        // Increment start of input data
        slice = &slice[written..];

        // We need to check if this is the last block as the header will then be
        // slightly different to indicate this.
        let last_block = deflate_state.lz77_state.is_last_block();

        let current_block_input_bytes = deflate_state.lz77_state
            .current_block_input_bytes();

        let res = {
            let (l_freqs, d_freqs) = deflate_state.lz77_writer.get_frequencies();
            gen_huffman_lengths(l_freqs, d_freqs, current_block_input_bytes)
        };

        // If the compressed representation is larger than the number of bytes
        // we write a stored block to avoid making the output larger than needed.
        match res {
            BlockType::Dynamic(header) => {
                // Write the block header.
                deflate_state.encoder_state
                    .write_start_of_block(false, last_block)?;

                // Output the lengths of the huffman codes used in this block.
                write_huffman_lengths(&header, &mut deflate_state.encoder_state.writer)?;

                // Output update the huffman table that will be used to encode the
                // lz77-compressed data.
                deflate_state.encoder_state
                    .update_huffman_table(&header.l_lengths, &header.d_lengths)?;


                // Write the huffman compressed data and the end of block marker.
                flush_to_bitstream(deflate_state.lz77_writer.get_buffer(),
                                   &mut deflate_state.encoder_state)?;
            }
            BlockType::Fixed => {
                // Write the block header for fixed code blocks.
                deflate_state.encoder_state.write_start_of_block(true, last_block)?;

                // Use the pre-defined static huffman codes.
                deflate_state.encoder_state.set_huffman_to_fixed()?;

                // Write the compressed data and the end of block marker.
                flush_to_bitstream(deflate_state.lz77_writer.get_buffer(),
                                   &mut deflate_state.encoder_state)?;
            }
            BlockType::Stored => {
                // Decompress the current lz77 encoded data to get back the
                // uncompressd bytes.
                // TODO: Avoid the temporary buffer here.
                let data = decompress_lz77_with_backbuffer(deflate_state.lz77_writer
                                                               .get_buffer(),
                                                           deflate_state.back_buffer
                                                               .get_buffer());

                write_stored_block(&data, deflate_state, flush == Flush::Finish && last_block)?;
            }
        };

        // If we are not done (or we are done but syncing), prepare for the next
        // block.
        if !(flush == Flush::Finish && status == LZ77Status::Finished) {

            // Clear the current lz77 data in the writer for the next call.
            deflate_state.lz77_writer.clear();
            // We are done with the block, so we reset the number of bytes taken
            // for the next one.
            deflate_state.lz77_state.reset_input_bytes();

            // Fill the back buffer.
            deflate_state.back_buffer
                .fill_buffer(&deflate_state.input_buffer.get_buffer()[..position]);

        }
        // We are done for now.
        if status == LZ77Status::Finished {
            break;
        }
    }

    // This flush mode means that there should be an empty stored block at the end.
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

        assert_eq!(test_data, result);
    }

    #[test]
    fn fixed_data() {
        let data = vec![190u8; 400];
        let compressed = compress_data_fixed(&data);
        let result = decompress_to_end(&compressed);

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
        println!("Fixed codes compressed len: {}", compressed.len());
        let result = decompress_to_end(&compressed);

        assert_eq!(input.len(), result.len());
        // Not using assert_eq here deliberately to avoid massive amounts of output spam.
        assert!(input == result);
    }
}
