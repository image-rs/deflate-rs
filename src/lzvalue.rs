use lz77::LDPair;

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

    pub fn distance(distance: u16) -> LZValue {
        LZValue { data: distance }
    }

    #[inline]
    pub fn value(&self) -> LDPair {
        match self.data & LITERAL_MASK {
            LITERAL_MASK => LDPair::Literal(self.data as u8),
            LENGTH_MASK => LDPair::Length(self.data & !LENGTH_MASK),
            _ => LDPair::Distance(self.data),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use lz77::LDPair;
    #[test]
    fn test_lzvalue() {
        use std::mem;
        println!("Size of lzvalue {}", mem::size_of::<LZValue>());
        let v = LZValue::literal(2);
        if let LDPair::Literal(n) = v.value() {
            assert_eq!(n, 2);
        } else {
            panic!();
        }

        let v = LZValue::length(55);
        if let LDPair::Length(n) = v.value() {
            assert_eq!(n, 55);
        } else {
            panic!();
        }

        let v = LZValue::distance(2555);
        if let LDPair::Distance(n) = v.value() {
            assert_eq!(n, 2555);
        } else {
            panic!();
        }
    }
}
