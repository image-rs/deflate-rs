use std::io::Write;
use std::io;

use deflate_state::DeflateState;
use checksum::{RollingChecksum, NoChecksum, Adler32Checksum};
use compression_options::CompressionOptions;
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

pub fn flush_to_bitstream<W: Write>(buffer: &[LZValue],
                                    state: &mut EncoderState<W>)
                                    -> io::Result<()> {
    for &b in buffer {
        state.write_ldpair(b.value())?
    }
    state.write_end_of_block()
}


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


pub fn compress_data_dynamic_n<W: Write>(input: &[u8],
                                         deflate_state: &mut DeflateState<W>,
                                         flush: bool)
                                         -> io::Result<usize> {

    let block_type = if flush {
        block_type_for_length(deflate_state.bytes_written + input.len())
    } else {
        BType::DynamicHuffman
    };

    let mut bytes_written = 0;

    match block_type {
        BType::DynamicHuffman | BType::FixedHuffman => {
            match block_type {
                BType::DynamicHuffman => {
                    let mut slice = input;
                    while !deflate_state.lz77_state.is_last_block() {
                        let (written, status) =
                            lz77_compress_block(&slice,
                                                &mut deflate_state.lz77_state,
                                                &mut deflate_state.input_buffer,
                                                &mut deflate_state.lz77_writer,
                                                flush);
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

            deflate_state.encoder_state
                .writer
                .write_bits(stored_block::STORED_FIRST_BYTE_FINAL.into(), 3)
                .unwrap();
            deflate_state.encoder_state.flush().unwrap();
            let written = compress_block_stored(input, &mut deflate_state.encoder_state.writer)?;
            bytes_written += written;
            deflate_state.bytes_written += written;
        }
    }

    deflate_state.encoder_state.flush().map(|()| bytes_written)
}


pub struct Compress<W: Write> {
    deflate_state: DeflateState<W>,
}

impl<W: Write> Compress<W> {
    fn new(input: &[u8], options: CompressionOptions, writer: W) -> Compress<W> {
        Compress { deflate_state: DeflateState::new(input, options, writer) }
    }
}

impl<W: Write> io::Write for Compress<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        compress_data_dynamic_n(buf, &mut self.deflate_state, false)
    }

    fn flush(&mut self) -> io::Result<()> {
        compress_data_dynamic_n(&[], &mut self.deflate_state, true).map(|_| ())
    }
}


#[cfg(test)]
mod test {
    use super::*;
    use compression_options::CompressionOptions;
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

    #[test]
    fn deflate_writer() {
        use std::io::Write;

        let mut compressed = Vec::with_capacity(32000);
        let data = get_test_data();
        {
            let mut compressor =
                Compress::new(&data, CompressionOptions::default(), &mut compressed);
            compressor.write(&data[0..37000]);
            compressor.write(&data[37000..]);
            compressor.flush();
        }
        println!("writer compressed len:{}", compressed.len());
        let res = decompress_to_end(&compressed);
        assert!(res == data);
    }
}
