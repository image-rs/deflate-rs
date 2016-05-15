#[cfg(test)]
extern crate flate2;

mod huffman_table;
mod lz77;
mod chained_hash_table;

use huffman_table::*;
use lz77::{LDPair, lz77_compress};

const BLOCK_SIZE: u16 = 32000;
const UNCOMPRESSED_FIRST_BYTE: u8 = 0b0000_0000;
const UNCOMPRESSED_FIRST_BYTE_FINAL: u8 = 0b0000_0001;
// TODO: Adding something in the unused bits here causes some issues
// Find out why
const FIXED_FIRST_BYTE: u16 = 0b0000_0010;
const FIXED_FIRST_BYTE_FINAL: u16 = 0b0000_0011;

pub enum BType {
    NoCompression = 0b00,
    FixedHuffman = 0b01,
    DynamicHuffman = 0b10, // Reserved = 0b11, //Error
}

/// A quick implementation of a struct that writes bit data to a buffer
struct BitWriter {
    bit_position: u8,
    accumulator: u32,
    // We currently write to a vector, but this might be
    // replaced with a writer later
    pub buffer: Vec<u8>,
}

impl BitWriter {
    fn new() -> BitWriter {
        BitWriter {
            bit_position: 0,
            accumulator: 0,
            buffer: Vec::new(),
        }
    }
    fn write_bits(&mut self, bits: u16, size: u8) {
        if size == 0 {
            return;
        }

        // self.accumulator |= (bits as u32) << (32 - size - self.bit_position);
        self.accumulator |= (bits as u32) << self.bit_position;
        self.bit_position += size;

        while self.bit_position >= 8 {
            // let byte = (self.accumulator >> 24) as u8;
            let byte = self.accumulator as u8;
            self.buffer.push(byte as u8);

            self.bit_position -= 8;
            // self.accumulator <<= 8;
            self.accumulator >>= 8;
        }
    }
    fn finish(&mut self) {
        if self.bit_position > 7 {
            // This should not happen.
            panic!("Error! Tried to finish bitwriter with more than 7 bits remaining!")
        }
        if self.bit_position != 0 {
            // println!("bit_position: {}, accumulator: {}", self.bit_position, self.accumulator);
            self.buffer.push(self.accumulator as u8);
        }
    }
}

struct EncoderState {
    huffman_table: huffman_table::HuffmanTable,
    writer: BitWriter,
}

impl EncoderState {
    fn new() -> EncoderState {
        EncoderState {
            huffman_table:
                huffman_table::HuffmanTable::from_length_tables(&FIXED_CODE_LENGTHS,
                                                                &FIXED_CODE_LENGTHS_DISTANCE)
                .unwrap(),
            writer: BitWriter::new(),
        }
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
                let ldencoded =
                    self.huffman_table.get_length_distance_code(length, distance).unwrap();
                self.writer.write_bits(ldencoded.length_code.code, ldencoded.length_code.length);
                self.writer.write_bits(ldencoded.length_extra_bits.code,
                                       ldencoded.length_extra_bits.length);
                self.writer
                    .write_bits(ldencoded.distance_code.code, ldencoded.distance_code.length);
                self.writer.write_bits(ldencoded.distance_extra_bits.code,
                                       ldencoded.distance_extra_bits.length);
            }
        };
    }

    /// Write the start of a block
    fn write_start_of_block(&mut self, final_block: bool) {
        if final_block {
            // The final block has one bit flipped to indicate it's
            // the final one one
            self.writer.write_bits(FIXED_FIRST_BYTE_FINAL, 3);
        } else {
            self.writer.write_bits(FIXED_FIRST_BYTE, 3);
        }
    }

    fn write_end_of_block(&mut self) {
        let code = self.huffman_table.get_end_of_block();
        // println!("End of block code: {:?}", code);
        self.writer.write_bits(code.code, code.length);
        // self.writer.finish();
    }

    /// Move and return the buffer from the writer
    pub fn take_buffer(&mut self) -> Vec<u8> {
        std::mem::replace(&mut self.writer.buffer, vec![])
    }

    pub fn flush(&mut self) {
        self.writer.finish();
    }
}

/// Split an u16 value into two bytes taking into account endianess
pub fn put16(value: u16) -> (u8, u8) {
    let value = u16::from_le(value);
    let low = value as u8;
    let high = (value >> 8) as u8;
    (low, high)
}

// Compress one block
pub fn compress_block_uncompressed(input: &[u8], final_block: bool) -> Vec<u8> {
    // First bit tells us if this is the final chunk
    let first_byte = if final_block {
        UNCOMPRESSED_FIRST_BYTE_FINAL
    } else {
        UNCOMPRESSED_FIRST_BYTE
    };

    println!("Chunk length: {}", input.len());

    // the next two details compression type (none in this case)
    let (len_0, len_1) = put16(input.len() as u16);
    // the next two after the length is the ones complement of the length
    let (not_len_0, not_len_1) = put16(!input.len() as u16);
    let mut output = vec![first_byte, len_0, len_1, not_len_0, not_len_1];
    output.extend_from_slice(input);
    output
}

pub fn compress_data_uncompressed(input: &[u8]) -> Vec<u8> {
    // TODO: Validate that block size is not too large
    let block_length = BLOCK_SIZE as usize;

    let mut output = Vec::new();
    let mut i = input.chunks(block_length).peekable();
    while let Some(chunk) = i.next() {
        let last_chunk = i.peek().is_none();
        output.extend(compress_block_uncompressed(chunk, last_chunk));
    }
    output
}


pub fn compress_data_fixed(input: &[u8]) -> Vec<u8> {
    // let block_length = 7;//BLOCK_SIZE as usize;

    let mut output = Vec::new();
    let mut state = EncoderState::new();
    let compressed = lz77_compress(input, chained_hash_table::WINDOW_SIZE).unwrap();

    state.write_start_of_block(true);
    for ld in compressed {
        state.write_ldpair(ld);
    }
    state.write_end_of_block();

    // let mut i = input.chunks(block_length).peekable();
    // while let Some(chunk) = i.next() {
    // let last_chunk = i.peek().is_none();
    //
    // state.write_start_of_block(last_chunk);
    // for byte in chunk {
    // state.write_literal(*byte);
    // }
    // state.write_end_of_block();
    // }

    state.flush();

    output.extend(state.take_buffer());
    output
}

pub fn compress_data(input: &[u8], btype: BType) -> Vec<u8> {
    match btype {
        BType::NoCompression => compress_data_uncompressed(input),
        BType::FixedHuffman => compress_data_fixed(input),
        BType::DynamicHuffman => panic!("ERROR: Dynamic huffman encoding not implemented yet!"),
    }
}

#[cfg(test)]
mod test {
    fn from_bytes(low: u8, high: u8) -> u16 {
        (low as u16) | ((high as u16) << 8)
    }

    /// Helper function to decompress into a `Vec<u8>`
    fn decompress_to_end(input: &[u8]) -> Vec<u8> {
        // let mut inflater = super::inflate::InflateStream::new();
        // let mut out = Vec::new();
        // let mut n = 0;
        // println!("input len {}", input.len());
        // while n < input.len() {
        // let (num_bytes_read, result) = inflater.update(&input[n..]).unwrap();
        // println!("result len {}, bytes_read {}", result.len(), num_bytes_read);
        // n += num_bytes_read;
        // out.extend(result);
        // }
        // out

        // Using flate2 instead of inflate, there seems to be some issue with inflate
        // for data longer than 399 bytes.
        use std::io::Read;
        use flate2::read::DeflateDecoder;

        let mut result = Vec::new();
        let mut e = DeflateDecoder::new(&input[..]);
        e.read_to_end(&mut result).unwrap();
        result
    }

    use super::*;
    #[test]
    fn test_bits() {
        let len = 520u16;
        let (low, high) = put16(len);
        assert_eq!(low, 8);
        assert_eq!(high, 2);

        let test2 = from_bytes(low, high);
        assert_eq!(len, test2);
    }

    #[test]
    fn test_no_compression_one_chunk() {
        let test_data = vec![1u8, 2, 3, 4, 5, 6, 7, 8];
        let compressed = compress_data(&test_data, BType::NoCompression);
        let result = decompress_to_end(&compressed);
        assert_eq!(test_data, result);
    }

    #[test]
    fn test_no_compression_multiple_chunks() {
        let test_data = vec![32u8; 40000];
        let compressed = compress_data(&test_data, BType::NoCompression);
        let result = decompress_to_end(&compressed);
        assert_eq!(test_data, result);
    }

    #[test]
    fn test_no_compression_string() {
        let test_data = String::from("This is some text, this is some more text, this is even \
                                      more text, lots of text here.")
            .into_bytes();
        let compressed = compress_data(&test_data, BType::NoCompression);
        let result = decompress_to_end(&compressed);
        assert_eq!(test_data, result);
    }

    #[test]
    fn test_fixed_string_mem() {
        use std::str;
        use std::io::Read;
        use flate2::read::DeflateDecoder;
        let test_data = String::from(".......................BB").into_bytes();
        let compressed = compress_data(&test_data, BType::FixedHuffman);
        // [0x73, 0x49, 0x4d, 0xcb, 0x49, 0x2c, 0x49, 0x55, 0xc8, 0x49, 0x2c, 0x49, 0x5, 0x0]

        // ==Flate2==
        {
            let mut e = DeflateDecoder::new(&compressed[..]);
            let mut result = Vec::new();
            e.read_to_end(&mut result).unwrap();
            let out_string = str::from_utf8(&result).unwrap();
            println!("Output: {}", out_string);
            assert_eq!(test_data, result);
        }

    }

    #[test]
    fn test_fixed_data() {

        let data = vec![190u8; 400];
        let compressed = compress_data(&data, BType::FixedHuffman);
        let result = decompress_to_end(&compressed);

        println!("data len: {}, result len: {}", data.len(), result.len());
        assert_eq!(data, result);
    }

    #[test]
    fn test_fixed_string_file() {
        use std::fs::File;
        use std::io::Read;
        let mut input = Vec::new();

        let mut f = File::open("src/gpl-3.0.txt").unwrap();

        f.read_to_end(&mut input).unwrap();
        let compressed = compress_data(&input, BType::FixedHuffman);
        let result = decompress_to_end(&compressed);
        assert_eq!(input, result);
    }

    #[test]
    fn test_writer() {
        let mut w = super::BitWriter::new();
        // w.write_bits(super::FIXED_FIRST_BYTE_FINAL, 3);
        w.write_bits(0b0111_0100, 8);
        w.write_bits(0, 8);
        println!("FIXED_FIRST_BYTE_FINAL: {:#b}",
                 super::FIXED_FIRST_BYTE_FINAL);
        println!("BIT: {:#b}", w.buffer[0]);
    }
}
