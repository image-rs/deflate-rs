// This was originally based on code from: https://github.com/nwin/lzw
// Copyright (c) 2015 nwin
// which is under both Apache 2.0 and MIT

//! This module provides a bit writer
use std::io::{self, Write};


///Writes bits to a byte stream, LSB first.
pub struct LsbWriter {
    // NOTE(oyvindln) Made this public for now so it can be replaced after initialization.
    pub w: Vec<u8>,
    bits: u8,
    acc: u32,
}

impl LsbWriter {
    /// Creates a new bit reader
    #[allow(dead_code)]
    pub fn new(writer: Vec<u8>) -> LsbWriter {
        LsbWriter {
            w: writer,
            bits: 0,
            acc: 0,
        }
    }

    pub fn pending_bits(&self) -> u8 {
        self.bits
    }

    pub fn write_bits(&mut self, v: u16, n: u8) {
        // NOTE: This outputs garbage data if n is 0, but v is not 0
        self.acc |= (v as u32) << self.bits;
        self.bits += n;
        while self.bits >= 8 {
            self.w.push(self.acc as u8);
            self.acc >>= 8;
            self.bits -= 8
        }
    }

    pub fn flush_raw(&mut self) {
        let missing = 8 - self.bits;
        // Have to test for self.bits > 0 here,
        // otherwise flush would output an extra byte when flush was called at a byte boundary
        if missing > 0 && self.bits > 0 {
            self.write_bits(0, missing);
        }
    }
}

impl Write for LsbWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.acc == 0 {
            self.w.extend_from_slice(buf)
        } else {
            for &byte in buf.iter() {
                self.write_bits(byte as u16, 8)
            }
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.flush_raw();
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::LsbWriter;

    #[test]
    fn write_bits() {
        let input = [
            (3, 3),
            (10, 8),
            (88, 7),
            (0, 2),
            (0, 5),
            (0, 0),
            (238, 8),
            (126, 8),
            (161, 8),
            (10, 8),
            (238, 8),
            (174, 8),
            (126, 8),
            (174, 8),
            (65, 8),
            (142, 8),
            (62, 8),
            (10, 8),
            (1, 8),
            (161, 8),
            (78, 8),
            (62, 8),
            (158, 8),
            (206, 8),
            (10, 8),
            (64, 7),
            (0, 0),
            (24, 5),
            (0, 0),
            (174, 8),
            (126, 8),
            (193, 8),
            (174, 8),
        ];
        let expected = [
            83,
            192,
            2,
            220,
            253,
            66,
            21,
            220,
            93,
            253,
            92,
            131,
            28,
            125,
            20,
            2,
            66,
            157,
            124,
            60,
            157,
            21,
            128,
            216,
            213,
            47,
            216,
        ];
        let mut writer = LsbWriter::new(Vec::new());
        for v in input.iter() {
            writer.write_bits(v.0, v.1);
        }
        assert_eq!(writer.w, expected);
    }
}


#[cfg(all(test, feature = "benchmarks"))]
mod bench {
    use test_std::Bencher;
    use super::LsbWriter;
    #[bench]
    fn bit_writer(b: &mut Bencher) {
        let input = [
            (3, 3),
            (10, 8),
            (88, 7),
            (0, 2),
            (0, 5),
            (0, 0),
            (238, 8),
            (126, 8),
            (161, 8),
            (10, 8),
            (238, 8),
            (174, 8),
            (126, 8),
            (174, 8),
            (65, 8),
            (142, 8),
            (62, 8),
            (10, 8),
            (1, 8),
            (161, 8),
            (78, 8),
            (62, 8),
            (158, 8),
            (206, 8),
            (10, 8),
            (64, 7),
            (0, 0),
            (24, 5),
            (0, 0),
            (174, 8),
            (126, 8),
            (193, 8),
            (174, 8),
        ];
        let mut writer = LsbWriter::new(Vec::with_capacity(100));
        b.iter(|| for v in input.iter() {
            let _ = writer.write_bits(v.0, v.1);
        });
    }
}
