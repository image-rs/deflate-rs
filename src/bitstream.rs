// Copyright (c) 2015 nwin
// This is copied from: https://github.com/nwin/lzw
// which is under both Apache 2.0 and MIT
// This should probably be made into a separate crate

//! This module provides a bit writer

use std::io::{self, Write};

/// A bit writer.
pub trait BitWriter: Write {
    /// Writes the next `n` bits.
    fn write_bits(&mut self, v: u16, n: u8) -> io::Result<()>;
}


///"Writes bits to a byte stream, LSB first."
///
///TODO: Simply use Vec<u8> here now.
pub struct LsbWriter<W: Write> {
    // NOTE(oyvindln) Made this public for now so it can be replaced after initialization.
    pub w: W,
    bits: u8,
    acc: u32,
}

impl<W: Write> LsbWriter<W> {
    /// Creates a new bit reader
    #[allow(dead_code)]
    pub fn new(writer: W) -> LsbWriter<W> {
        LsbWriter {
            w: writer,
            bits: 0,
            acc: 0,
        }
    }

    pub fn pending_bits(&self) -> u8 {
        self.bits
    }
}

impl<W: Write> Write for LsbWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.acc == 0 {
            self.w.write(buf)
        } else {
            for &byte in buf.iter() {
                try!(self.write_bits(byte as u16, 8))
            }
            Ok(buf.len())
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        let missing = 8 - self.bits;
        // NOTE:(oyvindln) Had to add a test for self.bits > 0 here,
        // otherwise flush would output an extra byte when flush was called at a byte boundary
        if missing > 0 && self.bits > 0 {
            try!(self.write_bits(0, missing));
        }
        self.w.flush()
    }
}

impl<W: Write> BitWriter for LsbWriter<W> {
    fn write_bits(&mut self, v: u16, n: u8) -> io::Result<()> {
        // NOTE:(oyvindln) This outputs garbage data if n is 0, but v is not 0
        self.acc |= (v as u32) << self.bits;
        self.bits += n;
        while self.bits >= 8 {
            //Ignore this as we only use it with vec at the moment, and
            //ignoring makes it faster.
            let _ = self.w.write_all(&[self.acc as u8]);
            self.acc >>= 8;
            self.bits -= 8
        }
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::{BitWriter, LsbWriter};

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
            writer.write_bits(v.0, v.1).unwrap();
        }
        assert_eq!(writer.w, expected);
    }
}


#[cfg(all(test, feature = "benchmarks"))]
mod bench {
    use test_std::Bencher;
    use super::{LsbWriter, BitWriter};
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
