/// A struct that writes bit data to a buffer
#[derive(Default)]
pub struct BitWriter {
    bit_position: u8,
    accumulator: u32,
    // We currently just write to a vector, but this should probably be
    // replaced with a writer later
    pub buffer: Vec<u8>,
}

impl BitWriter {
    pub fn new() -> BitWriter {
        BitWriter {
            bit_position: 0,
            accumulator: 0,
            buffer: Vec::new(),
        }
    }
    pub fn write_bits(&mut self, bits: u16, size: u8) {
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

    pub fn finish(&mut self) {
        if self.bit_position > 7 {
            // This should not happen.
            panic!("Error! Tried to finish bitwriter with more than 7 bits remaining!")
        }
        // Only do something if there actually are any bits left
        if self.bit_position != 0 {
            self.buffer.push(self.accumulator as u8);
        }
    }
}

// #[test]
//
// fn _test_writer() {
// let mut w = BitWriter::new();
// w.write_bits(super::FIXED_FIRST_BYTE_FINAL, 3);
// w.write_bits(0b0111_0100, 8);
// w.write_bits(0, 8);
// println!("FIXED_FIRST_BYTE_FINAL: {:#b}",
// super::FIXED_FIRST_BYTE_FINAL);
// println!("BIT: {:#b}", w.buffer[0]);
// }
//
