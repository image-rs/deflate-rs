use std::io;
use std::io::Write;
use huffman_table::{HuffmanTable, HuffmanError};
use bitstream::{LsbWriter, BitWriter};
use lz77::LDPair;

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

pub struct EncoderState<W: Write> {
    huffman_table: HuffmanTable,
    pub writer: LsbWriter<W>,
}

impl<W: Write> EncoderState<W> {
    pub fn new(huffman_table: HuffmanTable, writer: W) -> EncoderState<W> {
        EncoderState {
            huffman_table: huffman_table,
            writer: LsbWriter::new(writer),
        }
    }

    #[cfg(test)]
    pub fn fixed(writer: W) -> EncoderState<W> {
        EncoderState::new(HuffmanTable::fixed_table(), writer)
    }

    /// Encodes a literal value to the writer
    fn write_literal(&mut self, value: u8) -> io::Result<()> {
        let code = self.huffman_table.get_literal(value);
        self.writer.write_bits(code.code, code.length)
    }

    pub fn write_ldpair(&mut self, value: &LDPair) -> io::Result<()> {
        match *value {
            LDPair::Literal(l) => self.write_literal(l),
            LDPair::Length(l) => {
                let (code, extra_bits_code) = self.huffman_table.get_length_huffman(l).unwrap();
                try!(self.writer
                    .write_bits(code.code, code.length));
                self.writer.write_bits(extra_bits_code.code, extra_bits_code.length)
            }
            LDPair::Distance(d) => {
                let (code, extra_bits_code) = self.huffman_table
                    .get_distance_huffman(d)
                    .unwrap();

                try!(self.writer
                    .write_bits(code.code, code.length));
                self.writer.write_bits(extra_bits_code.code, extra_bits_code.length)
            }
        }
    }

    /// Write the start of a block
    pub fn write_start_of_block(&mut self, fixed: bool, final_block: bool) -> io::Result<()> {
        if final_block {
            // The final block has one bit flipped to indicate it's
            // the final one
            if fixed {
                self.writer.write_bits(FIXED_FIRST_BYTE_FINAL, 3)
            } else {
                self.writer.write_bits(DYNAMIC_FIRST_BYTE_FINAL, 3)
            }
        } else if fixed {
            self.writer.write_bits(FIXED_FIRST_BYTE, 3)
        } else {
            self.writer.write_bits(DYNAMIC_FIRST_BYTE, 3)
        }
    }

    pub fn write_end_of_block(&mut self) -> io::Result<()> {
        let code = self.huffman_table.get_end_of_block();
        self.writer.write_bits(code.code, code.length)
    }

    pub fn flush(&mut self) -> io::Result<()> {
        self.writer.flush()
    }

    pub fn update_huffman_table(&mut self,
                                literals_and_lengths: &[u8],
                                distances: &[u8])
                                -> Result<(), HuffmanError> {
        self.huffman_table.update_from_length_tables(literals_and_lengths, distances)
    }
}
