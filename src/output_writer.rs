use std::u16;

use lzvalue::LZValue;
use huffman_table::{NUM_LITERALS_AND_LENGTHS, NUM_DISTANCE_CODES, END_OF_BLOCK_POSITION,
                    get_distance_code, get_length_code};

/// The type used for representing how many times a literal, length or distance code has been ouput
/// to the current buffer.
/// As we are limiting the blocks to be at most 2^16 bytes long, we can represent frequencies using
/// 16-bit values.
pub type FrequencyType = u16;
/// The maximum number of literals/lengths in the buffer, which in practice also means the maximum
/// number of literals/lengths output before a new block is started.
/// This should not be larger than the maximum value `FrequencyType` can represent to prevent
/// overflowing (which would degrade, or in the worst case break compression).
pub const MAX_BUFFER_LENGTH: usize = u16::MAX as usize;

#[derive(Debug, PartialEq)]
pub enum BufferStatus {
    NotFull,
    Full,
}

/// A trait used by the lz77 compression function to write output.
/// Used to use the same function for compression with both fixed and dynamic huffman codes
/// (When fixed codes are used, there is no need to store frequency information)
pub trait OutputWriter {
    fn write_literal(&mut self, literal: u8) -> BufferStatus;
    fn write_length_distance(&mut self, length: u16, distance: u16) -> BufferStatus;
    fn buffer_length(&self) -> usize;
    fn clear_buffer(&mut self);
    fn get_buffer(&self) -> &[LZValue];
}

pub struct _DummyWriter {
    written: usize,
}

impl OutputWriter for _DummyWriter {
    fn write_literal(&mut self, _: u8) -> BufferStatus {
        self.written += 1;
        BufferStatus::NotFull
    }
    fn write_length_distance(&mut self, _: u16, _: u16) -> BufferStatus {
        self.written += 2;
        BufferStatus::NotFull
    }
    fn buffer_length(&self) -> usize {
        self.written
    }
    fn clear_buffer(&mut self) {}

    fn get_buffer(&self) -> &[LZValue] {
        &[]
    }
}

fn check_buffer_length(buffer: &[LZValue]) -> BufferStatus {
    if buffer.len() >= MAX_BUFFER_LENGTH {
        BufferStatus::Full
    } else {
        BufferStatus::NotFull
    }
}

/// `OutputWriter` that doesn't store frequency information
#[derive(Debug)]
pub struct FixedWriter {
    pub buffer: Vec<LZValue>,
}

impl FixedWriter {
    pub fn new() -> FixedWriter {
        FixedWriter { buffer: Vec::with_capacity(MAX_BUFFER_LENGTH) }
    }
}

impl OutputWriter for FixedWriter {
    fn write_literal(&mut self, literal: u8) -> BufferStatus {
        debug_assert!(self.buffer.len() < MAX_BUFFER_LENGTH);
        self.buffer.push(LZValue::literal(literal));
        check_buffer_length(&self.buffer)
    }

    fn write_length_distance(&mut self, length: u16, distance: u16) -> BufferStatus {
        debug_assert!(self.buffer.len() < MAX_BUFFER_LENGTH);
        self.buffer
            .push(LZValue::length_distance(length, distance));
        check_buffer_length(&self.buffer)
    }

    fn buffer_length(&self) -> usize {
        self.buffer.len()
    }

    fn clear_buffer(&mut self) {
        self.buffer.clear();
    }

    fn get_buffer(&self) -> &[LZValue] {
        &self.buffer
    }
}

// `OutputWriter` that keeps track of the usage of different codes
pub struct DynamicWriter {
    fixed_writer: FixedWriter,
    // The two last length codes are not actually used, but only participates in code construction
    // Therefore, we ignore them to get the correct number of lengths
    frequencies: [FrequencyType; NUM_LITERALS_AND_LENGTHS],
    distance_frequencies: [FrequencyType; NUM_DISTANCE_CODES],
}

impl OutputWriter for DynamicWriter {
    fn write_literal(&mut self, literal: u8) -> BufferStatus {
        let ret = self.fixed_writer.write_literal(literal);
        self.frequencies[usize::from(literal)] += 1;
        ret
    }

    fn write_length_distance(&mut self, length: u16, distance: u16) -> BufferStatus {
        let ret = self.fixed_writer.write_length_distance(length, distance);
        let l_code_num = get_length_code(length).expect("Invalid length!");
        // As we limit the buffer to 2^16 values, this should be safe from overflowing.
        self.frequencies[l_code_num] += 1;
        let d_code_num = get_distance_code(distance)
            .expect("Tried to get a distance code which was out of range!");
        self.distance_frequencies[usize::from(d_code_num)] += 1;
        ret
    }

    fn buffer_length(&self) -> usize {
        self.fixed_writer.buffer_length()
    }

    fn clear_buffer(&mut self) {
        self.clear_data();
        self.clear();
    }

    fn get_buffer(&self) -> &[LZValue] {
        self.fixed_writer.get_buffer()
    }
}

impl DynamicWriter {
    pub fn new() -> DynamicWriter {
        let mut w = DynamicWriter {
            fixed_writer: FixedWriter::new(),
            frequencies: [0; NUM_LITERALS_AND_LENGTHS],
            distance_frequencies: [0; NUM_DISTANCE_CODES],
        };
        // This will always be 1,
        // since there will always only be one end of block marker in each block
        w.frequencies[END_OF_BLOCK_POSITION] = 1;
        w
    }

    pub fn get_frequencies(&self) -> (&[u16], &[u16]) {
        (&self.frequencies, &self.distance_frequencies)
    }

    pub fn clear_frequencies(&mut self) {
        self.frequencies = [0; NUM_LITERALS_AND_LENGTHS];
        self.distance_frequencies = [0; NUM_DISTANCE_CODES];
        self.frequencies[END_OF_BLOCK_POSITION] = 1;
    }

    pub fn clear_data(&mut self) {
        self.fixed_writer.clear_buffer();
    }

    pub fn clear(&mut self) {
        self.clear_frequencies();
        self.clear_data();
    }
}
