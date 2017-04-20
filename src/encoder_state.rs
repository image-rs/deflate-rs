use std::io;
use std::io::{Write, ErrorKind};
use std::mem;
use huffman_table::{HuffmanTable, HuffmanError};
use bitstream::{LsbWriter, BitWriter};
use lzvalue::LZType;

// The first bits of each block, which describe the type of the block
// `-TTF` - TT = type, 00 = stored, 01 = fixed, 10 = dynamic, 11 = reserved, F - 1 if final block
// `0000`;
const FIXED_FIRST_BYTE: u16 = 0b010;
const FIXED_FIRST_BYTE_FINAL: u16 = 0b011;
const DYNAMIC_FIRST_BYTE: u16 = 0b100;
const DYNAMIC_FIRST_BYTE_FINAL: u16 = 0b101;

#[allow(dead_code)]
pub enum BType {
    NoCompression = 0b00,
    FixedHuffman = 0b01,
    DynamicHuffman = 0b10, // Reserved = 0b11, //Error
}

/// A struct wrapping a writer that writes data compressed using the provided huffman table
pub struct EncoderState<W: Write> {
    huffman_table: HuffmanTable,
    pub writer: LsbWriter<W>,
}

impl EncoderState<Vec<u8>> {
    pub fn inner_vec(&mut self) -> &mut Vec<u8> {
        &mut self.writer.w
    }
}

impl<W: Write> EncoderState<W> {
    /// Creates a new encoder state using the provided huffman table and writer
    pub fn new(huffman_table: HuffmanTable, writer: W) -> EncoderState<W> {
        EncoderState {
            huffman_table: huffman_table,
            writer: LsbWriter::new(writer),
        }
    }

    #[cfg(test)]
    /// Creates a new encoder state using the fixed huffman table
    pub fn fixed(writer: W) -> EncoderState<W> {
        EncoderState::new(HuffmanTable::fixed_table(), writer)
    }

    /// Encodes a literal value to the writer
    fn write_literal(&mut self, value: u8) -> io::Result<()> {
        let code = self.huffman_table.get_literal(value);
        debug_assert!(code.length > 0);
        self.writer.write_bits(code.code, code.length)
    }

    /// Write a LZvalue to the contained writer, returning Err if the write operation fails
    pub fn write_lzvalue(&mut self, value: LZType) -> io::Result<()> {
        match value {
            LZType::Literal(l) => self.write_literal(l),
            LZType::StoredLengthDistance(l, d) => {
                let (code, extra_bits_code) = self.huffman_table.get_length_huffman(l);
                self.writer.write_bits(code.code, code.length)?;
                self.writer
                    .write_bits(extra_bits_code.code, extra_bits_code.length)?;

                let (code, extra_bits_code) = self.huffman_table
                    .get_distance_huffman(d)
                    .ok_or_else(|| {
                                    io::Error::new(ErrorKind::Other,
                                                   "BUG!: Invalid huffman distance value!")
                                })?;

                self.writer.write_bits(code.code, code.length)?;
                self.writer
                    .write_bits(extra_bits_code.code, extra_bits_code.length)
            }
        }
    }

    /// Write the start of a block, returning Err if the write operation fails.
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

    /// Write the end of block code
    pub fn write_end_of_block(&mut self) -> io::Result<()> {
        let code = self.huffman_table.get_end_of_block();
        self.writer.write_bits(code.code, code.length)
    }

    /// Flush the contained writer and it's bitstream wrapper.
    pub fn flush(&mut self) -> io::Result<()> {
        self.writer.flush()
    }

    /// Update the huffman table by generating new huffman codes
    /// from length the length values from the provided tables.
    pub fn update_huffman_table(&mut self,
                                literals_and_lengths: &[u8],
                                distances: &[u8])
                                -> Result<(), HuffmanError> {
        self.huffman_table
            .update_from_length_tables(literals_and_lengths, distances)
    }

    pub fn set_huffman_to_fixed(&mut self) -> Result<(), HuffmanError> {
        self.huffman_table.set_to_fixed()
    }

    /// Reset the encoder state with a new writer, returning the old one if flushing
    /// succeeds.
    pub fn _reset(&mut self, writer: W) -> io::Result<W> {
        // Make sure the writer is flushed
        // Ideally this should be done before this function is called, but we
        // do it here just in case.
        self.flush()?;
        // Reset the huffman table
        // This probably isn't needed, but again, we do it just in case to avoid leaking any data
        // If this turns out to be a performance issue, it can probably be ignored later.
        self.huffman_table = HuffmanTable::empty();
        Ok(mem::replace(&mut self.writer.w, writer))
    }
}
