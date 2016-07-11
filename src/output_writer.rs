use lz77::LDPair;

pub trait OutputWriter {
    fn write_literal(&mut self, literal: u8);
    fn write_length_distance(&mut self, length: u16, distance: u16);
}

/// OutputWriter that doesn't store frequency information
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
