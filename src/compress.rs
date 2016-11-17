use std::io::Write;
use std::io;

use byteorder::{BigEndian, WriteBytesExt};

use deflate_state::DeflateState;
use checksum::{RollingChecksum, Adler32Checksum};
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
use zlib::{CompressionLevel, write_zlib_header};

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


pub struct DeflateEncoder<W: Write> {
    deflate_state: DeflateState<W>,
}

impl<W: Write> DeflateEncoder<W> {
    pub fn new(options: CompressionOptions, writer: W) -> DeflateEncoder<W> {
        DeflateEncoder { deflate_state: DeflateState::new(options, writer) }
    }
}

impl<W: Write> io::Write for DeflateEncoder<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        compress_data_dynamic_n(buf, &mut self.deflate_state, false)
    }

    fn flush(&mut self) -> io::Result<()> {
        compress_data_dynamic_n(&[], &mut self.deflate_state, true).map(|_| ())
    }
}

pub struct ZlibEncoder<W: Write> {
    deflate_state: DeflateState<W>,
    checksum: Adler32Checksum,
    header_written: bool,
}

impl<W: Write> ZlibEncoder<W> {
    pub fn new(options: CompressionOptions, writer: W) -> ZlibEncoder<W> {
        ZlibEncoder {
            deflate_state: DeflateState::new(options, writer),
            checksum: Adler32Checksum::new(),
            header_written: false,
        }
    }

    fn check_write_header(&mut self) -> io::Result<()> {
        if !self.header_written {
            write_zlib_header(&mut self.deflate_state.encoder_state.writer,
                              CompressionLevel::Default)?;
            self.header_written = true;
        }
        Ok(())
    }

    fn write_trailer(&mut self) -> io::Result<()> {
        let hash = self.checksum.current_hash();

        println!("Adler32: {}", hash);

        self.deflate_state
            .encoder_state
            .writer
            .write_u32::<BigEndian>(hash)
    }
}

impl<W: Write> io::Write for ZlibEncoder<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.check_write_header()?;
        self.checksum.update_from_slice(buf);
        compress_data_dynamic_n(buf, &mut self.deflate_state, false)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.check_write_header()?;
        compress_data_dynamic_n(&[], &mut self.deflate_state, true)?;
        self.write_trailer()
    }
}



#[cfg(test)]
mod test {
    use super::*;
    use test_utils::{get_test_data, decompress_to_end, decompress_zlib};
    use compression_options::CompressionOptions;


    #[test]
    fn deflate_writer() {
        use std::io::Write;

        let mut compressed = Vec::with_capacity(32000);
        let data = get_test_data();
        {
            let mut compressor = DeflateEncoder::new(CompressionOptions::high(), &mut compressed);
            compressor.write(&data[0..37000]).unwrap();
            compressor.write(&data[37000..]).unwrap();
            compressor.flush().unwrap();
        }
        println!("writer compressed len:{}", compressed.len());
        let res = decompress_to_end(&compressed);
        assert!(res == data);
    }



    #[test]
    fn zlib_writer() {
        use std::io::Write;

        let mut compressed = Vec::with_capacity(32000);
        let data = get_test_data();
        {
            let mut compressor =
                ZlibEncoder::new(CompressionOptions::high(), &mut compressed);
            compressor.write(&data[0..37000]).unwrap();
            compressor.write(&data[37000..]).unwrap();
            compressor.flush().unwrap();
        }
        println!("writer compressed len:{}", compressed.len());

        let res = decompress_zlib(&compressed);
        assert!(res == data);
    }


    #[test]
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
        // Not using assert_eq here deliberately to avoid massive amounts of output spam
        assert!(input == result);
    }
}
