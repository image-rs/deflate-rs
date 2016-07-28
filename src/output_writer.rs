use lz77::LDPair;
use huffman_table::NUM_LITERALS_AND_LENGTHS;

pub trait OutputWriter {
    fn write_literal(&mut self, literal: u8);
    fn write_length_distance(&mut self, length: u16, distance: u16);
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
}

pub struct DynamicWriter {
    fixed_writer: FixedWriter,
    frequenies: [u32; NUM_LITERALS_AND_LENGTHS],
}

impl DynamicWriter {
    pub fn new() -> DynamicWriter {
        DynamicWriter {
            fixed_writer: FixedWriter::new(),
            frequenies: [0; NUM_LITERALS_AND_LENGTHS],
        }
    }
}