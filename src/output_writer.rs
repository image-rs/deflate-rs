use lz77::LDPair;
use huffman_table::{NUM_LITERALS_AND_LENGTHS, NUM_DISTANCE_CODES, get_distance_code};

pub trait OutputWriter {
    fn write_literal(&mut self, literal: u8);
    fn write_length_distance(&mut self, length: u16, distance: u16);
    fn write_start_of_block(&mut self, is_final: bool);
}

/// OutputWriter that doesn't store frequency information
#[derive(Debug)]
pub struct FixedWriter {
    // TODO: Use a writer here instead
    pub buffer: Vec<LDPair>,
}

impl FixedWriter {
    pub fn new() -> FixedWriter {
        FixedWriter { buffer: Vec::new() }
    }
}

impl OutputWriter for FixedWriter {
    fn write_literal(&mut self, literal: u8) {
        self.buffer.push(LDPair::Literal(literal));
    }

    fn write_length_distance(&mut self, length: u16, distance: u16) {
        self.buffer.push(LDPair::LengthDistance {
            length: length,
            distance: distance,
        })
    }

    fn write_start_of_block(&mut self, is_final: bool) {
        self.buffer.push(LDPair::BlockStart { is_final: is_final });
    }
}

// OutputWriter that keeps track of the usage of different codes
pub struct DynamicWriter {
    fixed_writer: FixedWriter,
    // We may want to use u16 instead, depending on how large blocks
    // we want to use
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
        self.frequencies[length as usize] += 1;
        let d_code_num = get_distance_code(distance).expect("Error, distance is out of range!");
        self.frequencies[usize::from(d_code_num)] += 1;
    }

    fn write_start_of_block(&mut self, final_block: bool) {
        self.fixed_writer.write_start_of_block(final_block);
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
}
