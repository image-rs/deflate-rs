#[cfg(test)]
extern crate flate2;
//#[cfg(test)]
//extern crate inflate;

extern crate adler32;

mod huffman_table;
mod lz77;
mod chained_hash_table;
mod length_encode;
mod output_writer;
mod stored_block;
mod huffman_lengths;
mod bit_writer;
mod zlib;
mod checksum;
use huffman_table::*;
use lz77::{LDPair, lz77_compress};
use huffman_lengths::write_huffman_lengths;
use length_encode::huffman_lengths_from_frequency;
use bit_writer::BitWriter;
use checksum::RollingChecksum;

// The first bits of each block, which describe the type of the block
// `-TTF` - TT = type, 00 = stored, 01 = fixed, 10 = dynamic, 11 = reserved, F - 1 if final block
// `0000`;
const FIXED_FIRST_BYTE: u16 = 0b010;
const FIXED_FIRST_BYTE_FINAL: u16 = 0b011;
const DYNAMIC_FIRST_BYTE: u16 = 0b100;
const DYNAMIC_FIRST_BYTE_FINAL: u16 = 0b101;

pub enum BType {
    NoCompression = 0b00,
    FixedHuffman = 0b01,
    DynamicHuffman = 0b10, // Reserved = 0b11, //Error
}

// TODO: Use a trait here, and have implementations for each block type
struct EncoderState {
    huffman_table: huffman_table::HuffmanTable,
    writer: BitWriter,
    fixed: bool,
}

impl EncoderState {
    fn new(huffman_table: HuffmanTable) -> EncoderState {
        EncoderState {
            huffman_table: huffman_table,
            writer: BitWriter::new(),
            fixed: false,
        }
    }

    fn fixed() -> EncoderState {
        let mut ret = EncoderState::new(HuffmanTable::fixed_table());
        ret.fixed = true;
        ret
    }

    /// Encodes a literal value to the writer
    fn write_literal(&mut self, value: u8) {
        let code = self.huffman_table.get_literal(value);
        self.writer.write_bits(code.code, code.length);
    }

    fn write_ldpair(&mut self, value: LDPair) {
        match value {
            LDPair::Literal(l) => self.write_literal(l),
            LDPair::LengthDistance { length, distance } => {
                let ldencoded = self.huffman_table
                    .get_length_distance_code(length, distance)
                    .expect(&format!("Failed to get code for length: {}, distance: {}",
                                     length,
                                     distance));
                self.writer.write_bits(ldencoded.length_code.code, ldencoded.length_code.length);
                self.writer.write_bits(ldencoded.length_extra_bits.code,
                                       ldencoded.length_extra_bits.length);
                self.writer
                    .write_bits(ldencoded.distance_code.code, ldencoded.distance_code.length);
                self.writer.write_bits(ldencoded.distance_extra_bits.code,
                                       ldencoded.distance_extra_bits.length);
            }
            LDPair::BlockStart{..} => {
                panic!("Tried to write start of block, this should not be handled here!");
            },
            LDPair::EndOfBlock => {
                self.write_end_of_block();
            }
        };
    }

    /// Write the start of a block
    fn write_start_of_block(&mut self, final_block: bool) {
        if final_block {
            // The final block has one bit flipped to indicate it's
            // the final one
            if self.fixed {
                self.writer.write_bits(FIXED_FIRST_BYTE_FINAL, 3);
            } else {
                self.writer.write_bits(DYNAMIC_FIRST_BYTE_FINAL, 3);
            }
        } else if self.fixed {
            self.writer.write_bits(FIXED_FIRST_BYTE, 3);
        } else {
            self.writer.write_bits(DYNAMIC_FIRST_BYTE, 3);
        }
    }

    fn write_end_of_block(&mut self) {
        let code = self.huffman_table.get_end_of_block();
        self.writer.write_bits(code.code, code.length);
    }

    /// Move and return the buffer from the writer
    pub fn take_buffer(&mut self) -> Vec<u8> {
        std::mem::replace(&mut self.writer.buffer, vec![])
    }

    pub fn flush(&mut self) {
        self.writer.finish();
    }

    pub fn set_huffman_table(&mut self, table: huffman_table::HuffmanTable) {
        self.huffman_table = table;
    }
}

fn compress_data_fixed(input: &[u8]) -> Vec<u8> {
    let mut output = Vec::new();
    let mut state = EncoderState::fixed();
    let compressed = lz77_compress(input, chained_hash_table::WINDOW_SIZE).unwrap();
    let clen = compressed.len();

    //We currently don't split blocks, we should do this eventually
    state.write_start_of_block(true);
    for ld in compressed {
        //We ignore end of block here for now since there is no purpose of
        //splitting a full stream of data using fixed huffman data into blocks
        match ld {
            LDPair::BlockStart{..}|LDPair::EndOfBlock =>
            (),
                _ => state.write_ldpair(ld),
        }
    }

    state.write_end_of_block();
    state.flush();

    output.extend(state.take_buffer());
    println!("Input length: {}, Compressed len: {}, Output length: {}",
             input.len(),
             clen,
             output.len());
    output
}

fn compress_data_dynamic<RC: RollingChecksum>(input: &[u8]) -> (Vec<u8>, u32) {
    let mut output = Vec::new();
    let mut state = EncoderState::new(huffman_table::HuffmanTable::empty());

    let mut lz77_state = lz77::LZ77State::new(input);
    let mut lz77_writer = output_writer::DynamicWriter::new();
    let mut checksum = RC::new();
    checksum.update_from_slice(&input[..2]);

    while !lz77_state.is_last_block() {
        lz77::lz77_compress_block(input, &mut lz77_state, &mut lz77_writer, &mut checksum);
        state.write_start_of_block(lz77_state.is_last_block());

        let (l_lengths, d_lengths) = {
            let (l_freqs, d_freqs) = lz77_writer.get_frequencies();

            (huffman_lengths_from_frequency(l_freqs, MAX_CODE_LENGTH),
             huffman_lengths_from_frequency(d_freqs, MAX_CODE_LENGTH))
        };
        write_huffman_lengths(&l_lengths, &d_lengths, &mut state.writer);
        let codes = HuffmanTable::from_length_tables(&l_lengths, &d_lengths).expect(
            "Error: Failed to create huffman table!"
        );
        state.set_huffman_table(codes);

        for ld in lz77_writer.get_buffer() {
            match *ld {
                LDPair::BlockStart{..} => (),
                _ =>  state.write_ldpair(*ld),
            }
        }
        // End of block is written in write_ldpair

        lz77_writer.clear();
    }

    state.flush();

    output.extend(state.take_buffer());

    (output, checksum.current_hash())
}

pub fn deflate_bytes(input: &[u8]) -> Vec<u8> {
    let (data, _) = compress_data_dynamic::<checksum::NoChecksum>(input);
    data
}

pub fn deflate_bytes_zlib(input: &[u8]) -> Vec<u8> {
    // Temporary doing this in a hacky way for testing purposes
    let (mut data, checksum) = compress_data_dynamic::<checksum::Adler32Checksum>(input);
    data.insert(0, 1);
    data.insert(0, 120);

    data.push((checksum >> 24) as u8);
    data.push((checksum >> 16) as u8);
    data.push((checksum >> 8) as u8);
    data.push(checksum as u8);
    data
}

#[cfg(test)]
mod test {
    use stored_block::compress_data_stored;
    use super::compress_data_fixed;

    /// Helper function to decompress into a `Vec<u8>`
    fn decompress_to_end(input: &[u8]) -> Vec<u8> {
        /*use std::str;
        let mut inflater = super::inflate::InflateStream::new();
        let mut out = Vec::new();
        let mut n = 0;
        println!("input len {}", input.len());
        while n < input.len() {
            let res = inflater.update(&input[n..]) ;
            if let Ok((num_bytes_read, result)) = res {
                println!("result len {}, bytes_read {}", result.len(), num_bytes_read);
                n += num_bytes_read;
                out.extend(result);
            } else {
                //println!("Output: `{}`", str::from_utf8(&out).unwrap());
                println!("Output decompressed: {}", out.len());
                res.unwrap();
            }

        }
        out*/

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
        // let test_data = b".......................BB";
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
        use std::fs::File;
        use std::io::Read;
        use std::str;
        let mut input = Vec::new();

        let mut f = File::open("src/pg11.txt").unwrap();

        f.read_to_end(&mut input).unwrap();
        let compressed = compress_data_fixed(&input);
        println!("Compressed len: {}", compressed.len());
        let result = decompress_to_end(&compressed);
        let out1 = str::from_utf8(&input).unwrap();
        let out2 = str::from_utf8(&result).unwrap();
        // println!("Orig:\n{}", out1);
        // println!("Compr:\n{}", out2);
        println!("Orig len: {}, out len: {}", out1.len(), out2.len());
        // Not using assert_eq here deliberately to avoid massive amounts of output spam
        assert!(input == result);
    }



    #[test]
    fn test_dynamic_string_mem() {
        use std::str;
        let test_data = String::from("                    GNU GENERAL PUBLIC LICENSE").into_bytes();
        let compressed = deflate_bytes(&test_data);

        let result = decompress_to_end(&compressed);
        assert_eq!(test_data, result);
    }

    #[test]
    fn test_dynamic_string_file() {
        use std::fs::File;
        use std::io::Read;
        use std::str;
        let mut input = Vec::new();

        let mut f = File::open("src/pg11.txt").unwrap();

        f.read_to_end(&mut input).unwrap();
        let compressed = deflate_bytes(&input);

        println!("Compressed len: {}", compressed.len());

        let result = decompress_to_end(&compressed);
        // Check that we actually managed to compress the input
        assert!(compressed.len() < input.len());
        // Not using assert_eq here deliberately to avoid massive amounts of output spam
        assert!(input == result);
    }

    #[test]
    fn test_dynamic_string_zlib() {
        use std::io::Read;
        use flate2::read::ZlibDecoder;
        let test_data = String::from("foo zdsujghns aaaaaa hello hello eshtguiq3ayth932wa7tyh13a79hgqae78guh").into_bytes();
        let compressed = deflate_bytes_zlib(&test_data);

        let mut e = ZlibDecoder::new(&compressed[..]);

        let mut result = Vec::new();
        let _ = e.read_to_end(&mut result).unwrap();
        assert_eq!(&test_data, &result);
    }
}
