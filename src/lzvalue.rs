use lz77::LDPair;
use huffman_table::MAX_DISTANCE;

const LITERAL_MASK: u16 = 0b1100_0000_0000_0000;
const LENGTH_MASK: u16 = 0b1000_0000_0000_0000;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct LZValue {
    data: u16,
}

impl LZValue {
    pub fn literal(value: u8) -> LZValue {
        LZValue { data: value as u16 ^ LITERAL_MASK }
    }

    pub fn length(length: u16) -> LZValue {
        // We prob want to use length - 3 here
        LZValue { data: length ^ LENGTH_MASK }
    }

    pub fn distance(mut distance: u16) -> LZValue {
        if distance == MAX_DISTANCE {
            distance = 0;
        }
        LZValue { data: distance }
    }

    #[inline]
    pub fn value(&self) -> LDPair {
        match self.data & LITERAL_MASK {
            LITERAL_MASK => LDPair::Literal(self.data as u8),
            LENGTH_MASK => LDPair::Length(self.data & !LENGTH_MASK),
            _ => {
                if self.data == 0 {
                    LDPair::Distance(MAX_DISTANCE)
                } else {
                    LDPair::Distance(self.data)
                }
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use lz77::LDPair;
    use huffman_table::{MIN_MATCH, MIN_DISTANCE, MAX_MATCH, MAX_DISTANCE};
    #[test]
    fn test_lzvalue() {
        for i in 0..255 as usize + 1 {
            let v = LZValue::literal(i as u8);
            if let LDPair::Literal(n) = v.value() {
                assert_eq!(n as usize, i);
            } else {
                panic!();
            }
        }

        for i in MIN_MATCH..MAX_MATCH + 1 {
            let v = LZValue::length(i);
            if let LDPair::Length(n) = v.value() {
                assert_eq!(n, i);
            } else {
                panic!();
            }
        }

        for i in MIN_DISTANCE..MAX_DISTANCE + 1 {
            let v = LZValue::distance(i);
            if let LDPair::Distance(n) = v.value() {
                assert_eq!(n, i);
            } else {
                panic!("Failed to get distance {}", i);
            }
        }

    }
}
