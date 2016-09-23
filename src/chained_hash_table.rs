pub const WINDOW_SIZE: usize = 32768;
pub const WINDOW_MASK: usize = WINDOW_SIZE - 1;
#[allow(dead_code)]
pub const HASH_BYTES: usize = 3;
/// The number of bits to shift on each hash update. Not sure if 5 or 6 is better.
const HASH_SHIFT: u32 = 5;
const HASH_MASK: u32 = WINDOW_MASK as u32;

/// Returns a new hash value based on the previous value and the next byte
fn update_hash(current_hash: u32, to_insert: u8, shift: u32, mask: u32) -> u32 {
    ((current_hash << shift) ^ (to_insert as u32)) & mask
}

pub struct ChainedHashTable {
    // Current running hash value of the last 3 bytes
    current_hash: u32,
    // The current position
    current_pos: usize,
    // Starts of hash chains (in prev)
    head: Box<[u16; WINDOW_SIZE]>,
    // link to previous occurence of this hash value
    prev: Box<[u16; WINDOW_SIZE]>,
}

impl ChainedHashTable {
    fn new() -> ChainedHashTable {
        ChainedHashTable {
            current_hash: 0,
            current_pos: 0,
            head: Box::new([0; WINDOW_SIZE]),
            prev: Box::new([0; WINDOW_SIZE]),
        }
    }

    pub fn from_starting_values(v1: u8, v2: u8) -> ChainedHashTable {
        let mut t = ChainedHashTable::new();
        t.add_hash_value(0, v1);
        t.add_hash_value(1, v2);
        t
    }

    // Insert a byte into the hash table
    pub fn add_hash_value(&mut self, position: usize, value: u8) {
        // TODO: Do we need to allow different shifts/masks?
        self.current_hash = update_hash(self.current_hash, value, HASH_SHIFT, HASH_MASK);
        self.prev[position & WINDOW_MASK] = self.current_head();
        self.head[self.current_hash as usize] = position as u16;
        self.current_pos = position;
    }

    // Get the head of the hash chain of the current hash value
    #[inline]
    pub fn current_head(&self) -> u16 {
        self.head[self.current_hash as usize]
    }

    #[cfg(test)]
    #[inline]
    pub fn current_position(&self) -> usize {
        self.current_pos
    }

    #[inline]
    pub fn get_prev(&self, bytes: usize) -> u16 {
        self.prev[bytes & WINDOW_MASK]
    }

    pub fn _current_hash(&self) -> u32 {
        self.current_hash
    }

    #[allow(dead_code)]
    fn slide_value(b: u16, bytes: u16) -> u16 {
        if b >= bytes { b - bytes } else { 0 }
    }

    pub fn slide(&mut self, bytes: usize) {
        for b in &mut self.head.iter_mut() {
            *b = ChainedHashTable::slide_value(*b, bytes as u16);
        }

        for b in &mut self.prev.iter_mut() {
            *b = ChainedHashTable::slide_value(*b, bytes as u16);
        }
    }
}

#[cfg(test)]
pub fn filled_hash_table(data: &[u8]) -> ChainedHashTable {
    let mut hash_table = ChainedHashTable::from_starting_values(data[0], data[1]);
    for (n, b) in data[2..].iter().enumerate() {
        hash_table.add_hash_value(n, *b);
    }
    hash_table
}

#[cfg(test)]
mod test {
    #[test]
    fn test_chained_hash() {
        use std::str;

        let test_string = "Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do \
                           eiusmod tempor. rum. incididunt ut labore et dolore magna aliqua. Ut \
                           enim ad minim veniam, quis nostrud exercitation ullamco laboris nisi \
                           ut aliquip ex ea commodo consequat. rum. Duis aute irure dolor in \
                           reprehenderit in voluptate velit esse cillum dolore eu fugiat nulla \
                           pariatur. Excepteur sint occaecat cupidatat non proident, sunt in \
                           culpa qui officia deserunt mollit anim id est laborum.";

        let test_data = test_string.as_bytes();

        let current_bytes = &test_data[test_data.len() - super::HASH_BYTES..test_data.len()];

        let num_iters = test_string.matches(str::from_utf8(current_bytes).unwrap())
            .count();

        let hash_table = super::filled_hash_table(test_data);

        // Test that the positions in the chain are valid
        let mut prev_value = hash_table.current_head() as usize;
        let mut count = 0;
        while prev_value > 1 {
            assert_eq!(current_bytes,
                       &test_data[prev_value..prev_value + super::HASH_BYTES]);
            count += 1;
            prev_value = hash_table.get_prev(prev_value) as usize;
        }
        assert_eq!(count, num_iters);
    }

    #[test]
    fn test_table_unique() {
        let mut test_data = Vec::new();
        test_data.extend((0u8..255));
        test_data.extend((255u8..0));
        let hash_table = super::filled_hash_table(&test_data);
        let prev_pos = hash_table.get_prev(hash_table.current_head() as usize);
        // Since all sequences in the input are unique, there shouldn't be any previous values
        assert_eq!(prev_pos, 0);
    }

    #[test]
    fn test_table_slide() {
        use std::fs::File;
        use std::io::Read;
        use std::str;

        let window_size = super::WINDOW_SIZE;
        let window_size16 = super::WINDOW_SIZE as u16;

        let mut input = Vec::new();

        let mut f = File::open("tests/pg11.txt").unwrap();

        f.read_to_end(&mut input).unwrap();

        let mut hash_table = super::filled_hash_table(&input[..window_size]);
        for (n, b) in input[..window_size].iter().enumerate() {
            hash_table.add_hash_value(n + window_size, *b);
        }

        hash_table.slide(window_size);

        {
            let max_head = hash_table.head.iter().max().unwrap();
            // After sliding there should be no hashes referring to values
            // higher than the window size
            assert!(*max_head < window_size16);
            assert!(*max_head > 0);
            let mut pos = hash_table.current_head();
            // There should be a previous occurence since we inserted the data 3 times
            assert!(pos < window_size16);
            assert!(pos > 0);
            let end_byte = input[window_size - 1];
            while pos > 0 {
                assert_eq!(input[pos as usize & window_size - 1], end_byte);
                pos = hash_table.get_prev(pos as usize);
            }

        }

        for (n, b) in input[..(window_size / 2)].iter().enumerate() {
            hash_table.add_hash_value(n + window_size, *b);
        }

        // There should hashes referring to values in the upper part of the input window
        // at this point
        let max_prev = hash_table.prev.iter().max().unwrap();
        assert!(*max_prev > window_size16);

        let mut pos = hash_table.current_head();
        // There should be a previous occurence since we inserted the data 3 times
        assert!(pos > window_size16);
        let end_byte = input[(window_size / 2) - 1];
        let mut iterations = 0;
        while pos > window_size16 && iterations < 5000 {
            assert_eq!(input[pos as usize & window_size - 1], end_byte);

            pos = hash_table.get_prev(pos as usize);
            iterations += 1;
        }
    }
}
