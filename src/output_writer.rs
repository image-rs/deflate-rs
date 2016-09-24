use lz77::LDPair;
use huffman_table::{NUM_LITERALS_AND_LENGTHS, NUM_DISTANCE_CODES, END_OF_BLOCK_POSITION,
                    get_distance_code, get_length_code};

/// A trait used by the lz77 compression function to write output.
/// Used to use the same function for compression with both fixed and dynamic huffman codes
/// (When fixed codes are used, there is no need to store frequency information)
pub trait OutputWriter {
    fn write_literal(&mut self, literal: u8);
    fn write_length_distance(&mut self, length: u16, distance: u16);
    fn write_end_of_block(&mut self);
}

/// `OutputWriter` that doesn't store frequency information
#[derive(Debug)]
pub struct FixedWriter {
    // TODO: Use a writer here instead
    pub buffer: Vec<LDPair>,
}

impl FixedWriter {
    pub fn new() -> FixedWriter {
        FixedWriter { buffer: Vec::with_capacity(10000) }
    }

    pub fn clear_buffer(&mut self) {
        self.buffer.clear();
    }
}

impl OutputWriter for FixedWriter {
    fn write_literal(&mut self, literal: u8) {
        self.buffer.push(LDPair::Literal(literal));
    }

    fn write_length_distance(&mut self, length: u16, distance: u16) {
        self.buffer.push(LDPair::length(length));
        self.buffer.push(LDPair::distance(distance));
    }

    fn write_end_of_block(&mut self) {
        self.buffer.push(LDPair::EndOfBlock);
    }
}

// `OutputWriter` that keeps track of the usage of different codes
pub struct DynamicWriter {
    fixed_writer: FixedWriter,
    // We may want to use u16 instead, depending on how large blocks
    // we want to use
    // The two last length codes are not actually used, but only participates in code construction
    // Therefore, we ignore them to get the correct number of lengths
    frequencies: [u32; NUM_LITERALS_AND_LENGTHS],
    distance_frequencies: [u32; NUM_DISTANCE_CODES],
}

impl OutputWriter for DynamicWriter {
    fn write_literal(&mut self, literal: u8) {
        self.fixed_writer.write_literal(literal);
        self.frequencies[usize::from(literal)] += 1;
    }

    fn write_length_distance(&mut self, length: u16, distance: u16) {
        self.fixed_writer.write_length_distance(length, distance);
        let l_code_num = get_length_code(length).expect("Invalid length!");
        self.frequencies[l_code_num as usize] += 1;
        let d_code_num = get_distance_code(distance).expect("Error, distance is out of range!");
        self.distance_frequencies[usize::from(d_code_num)] += 1;
    }

    fn write_end_of_block(&mut self) {
        self.frequencies[END_OF_BLOCK_POSITION] += 1;
        self.fixed_writer.write_end_of_block();
    }
}

impl DynamicWriter {
    pub fn new() -> DynamicWriter {
        DynamicWriter {
            fixed_writer: FixedWriter::new(),
            frequencies: [0; NUM_LITERALS_AND_LENGTHS],
            distance_frequencies: [0; NUM_DISTANCE_CODES],
        }
    }

    pub fn get_frequencies(&self) -> (&[u32], &[u32]) {
        (&self.frequencies, &self.distance_frequencies)
    }

    pub fn clear_frequencies(&mut self) {
        self.frequencies = [0; NUM_LITERALS_AND_LENGTHS];
        self.distance_frequencies = [0; NUM_DISTANCE_CODES];
    }

    pub fn clear_data(&mut self) {
        self.fixed_writer.clear_buffer();
    }

    pub fn clear(&mut self) {
        self.clear_frequencies();
        self.clear_data();
    }

    pub fn get_buffer(&mut self) -> &[LDPair] {
        &self.fixed_writer.buffer
    }
}
