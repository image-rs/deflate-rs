pub const WINDOW_SIZE: usize = 32768;
const WINDOW_MASK: usize = WINDOW_SIZE - 1;
#[cfg(test)]
pub const HASH_BYTES: usize = 3;
const HASH_SHIFT: u16 = 5;
const HASH_MASK: u16 = WINDOW_MASK as u16;

/// Returns a new hash value based on the previous value and the next byte
#[inline]
fn update_hash(current_hash: u16, to_insert: u8) -> u16 {
    update_hash_conf(current_hash, to_insert, HASH_SHIFT, HASH_MASK)
}

#[inline]
fn update_hash_conf(current_hash: u16, to_insert: u8, shift: u16, mask: u16) -> u16 {
    ((current_hash << shift) ^ (to_insert as u16)) & mask
}

#[inline]
fn init_array(arr: &mut [u16]) {
    for (n, mut b) in arr.iter_mut().enumerate() {
        *b = n as u16;
    }
}

fn new_array() -> Box<[u16]> {
    // Create the vector with the elements initialised as using collect or extend ends
    // up being significantly slower for some reason.
    let mut arr = vec![0;WINDOW_SIZE];
    init_array(&mut arr);
    arr.into_boxed_slice()
}

pub struct ChainedHashTable {
    // Current running hash value of the last 3 bytes
    current_hash: u16,
    // Starts of hash chains (in prev)
    head: Box<[u16]>,
    // link to previous occurence of this hash value
    prev: Box<[u16]>,
    // Used for testing
    // Didn't find an easy way for it not to exist when debug_assertions are disabled.
    pub count: u64,
}

impl ChainedHashTable {
    pub fn new() -> ChainedHashTable {
        let chain = new_array();
        ChainedHashTable {
            current_hash: 0,
            head: chain.clone(),
            prev: chain,
            count: 0,
        }
    }

    #[cfg(test)]
    pub fn from_starting_values(v1: u8, v2: u8) -> ChainedHashTable {
        let mut t = ChainedHashTable::new();
        t.current_hash = update_hash(t.current_hash, v1);
        t.current_hash = update_hash(t.current_hash, v2);
        t
    }

    /// Resets the hash value and hash chains
    pub fn reset(&mut self) {
        self.current_hash = 0;
        init_array(&mut self.head);
        init_array(&mut self.prev);
        if cfg!(debug_assertions) {
            self.count = 0;
        }
    }

    pub fn add_initial_hash_values(&mut self, v1: u8, v2: u8) {
        self.current_hash = update_hash(self.current_hash, v1);
        self.current_hash = update_hash(self.current_hash, v2);
    }

    // Insert a byte into the hash table
    pub fn add_hash_value(&mut self, position: usize, value: u8) {
        // Check that all bytes are input in order and at the correct positions.
        debug_assert_eq!(position & WINDOW_MASK, self.count as usize & WINDOW_MASK);
        debug_assert!(position < WINDOW_SIZE * 2,
                      "Position is larger than 2 * window size! {}",
                      position);
        // Storing the hash in a temporary variable here makes the compiler avoid the
        // bounds checks in this function.
        let new_hash = update_hash(self.current_hash, value);

        self.prev[position & WINDOW_MASK] = self.head[new_hash as usize];

        // Ignoring any bits over 16 here is deliberate, as we only concern ourselves about
        // where in the buffer (which is 64k bytes) we are referring to.
        self.head[new_hash as usize] = position as u16;

        // Update the stored hash value with the new hash.
        self.current_hash = new_hash;

        if cfg!(debug_assertions) {
            self.count += 1;
        }
    }

    // Get the head of the hash chain for the current hash value
    #[cfg(test)]
    #[inline]
    pub fn current_head(&self) -> u16 {
        self.head[self.current_hash as usize]
    }

    #[cfg(test)]
    #[inline]
    pub fn current_hash(&self) -> u16 {
        self.current_hash
    }

    #[inline]
    pub fn get_prev(&self, bytes: usize) -> u16 {
        self.prev[bytes & WINDOW_MASK]
    }

    fn slide_value(b: u16, pos: u16, bytes: u16) -> u16 {
        if b >= bytes { b - bytes } else { pos }
    }

    fn slide_table(table: &mut [u16], bytes: u16) {
        for (n, b) in table.iter_mut().enumerate() {
            *b = ChainedHashTable::slide_value(*b, n as u16, bytes);
        }
    }

    pub fn slide(&mut self, bytes: usize) {
        if cfg!(debug_assertions) {
            if bytes != WINDOW_SIZE {
                // This should only happen in tests in this file.
                self.count = 0;
            }
        }
        ChainedHashTable::slide_table(&mut self.head[..], bytes as u16);
        ChainedHashTable::slide_table(&mut self.prev[..], bytes as u16);
    }

    // #[cfg(test)]
    pub fn _get_head_arr(&self) -> &[u16] {
        &self.head[..]
    }

    // #[cfg(test)]
    pub fn _get_prev_arr(&self) -> &[u16] {
        &self.prev[..]
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
    use super::filled_hash_table;

    #[test]
    fn chained_hash() {
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

        let num_iters = test_string
            .matches(str::from_utf8(current_bytes).unwrap())
            .count();

        let hash_table = filled_hash_table(test_data);

        // Test that the positions in the chain are valid
        let mut prev_value = hash_table.get_prev(hash_table.current_head() as usize) as usize;
        let mut count = 0;
        let mut current = hash_table.current_head() as usize;
        while current != prev_value {
            count += 1;
            current = prev_value;
            prev_value = hash_table.get_prev(prev_value) as usize;
        }
        // There should be at least as many occurences of the hash of the checked bytes as the
        // numbers of occurences of the checked bytes themselves. As the hashes are not large enough
        // to store 8 * 3 = 24 bits, there could be more with different input data.
        assert!(count >= num_iters);
    }

    #[test]
    fn table_unique() {
        let mut test_data = Vec::new();
        test_data.extend((0u8..255));
        test_data.extend((255u8..0));
        let hash_table = filled_hash_table(&test_data);
        let prev_pos = hash_table.get_prev(hash_table.current_head() as usize);
        // Since all sequences in the input are unique, there shouldn't be any previous values
        assert_eq!(prev_pos, hash_table.current_hash());
    }

    #[test]
    fn table_slide() {
        use std::fs::File;
        use std::io::Read;
        use std::str;

        let window_size = super::WINDOW_SIZE;
        let window_size16 = super::WINDOW_SIZE as u16;

        let mut input = Vec::new();

        let mut f = File::open("tests/pg11.txt").unwrap();

        f.read_to_end(&mut input).unwrap();

        let mut hash_table = filled_hash_table(&input[..window_size + 2]);

        for (n, b) in input[2..window_size + 2].iter().enumerate() {
            hash_table.add_hash_value(n + window_size, *b);
        }

        hash_table.slide(window_size);

        {
            let max_head = hash_table.head.iter().max().unwrap();
            // After sliding there should be no hashes referring to values
            // higher than the window size
            assert!(*max_head < window_size16);
            assert!(*max_head > 0);
            let pos = hash_table.get_prev(hash_table.current_head() as usize);
            // There should be a previous occurence since we inserted the data 3 times
            assert!(pos < window_size16);
            assert!(pos > 0);
        }

        for (n, b) in input[2..(window_size / 2)].iter().enumerate() {
            hash_table.add_hash_value(n + window_size, *b);
        }

        // There should hashes referring to values in the upper part of the input window
        // at this point
        let max_prev = hash_table.prev.iter().max().unwrap();
        assert!(*max_prev > window_size16);

        let mut pos = hash_table.current_head();
        // There should be a previous occurence since we inserted the data 3 times
        assert!(pos > window_size16);
        let end_byte = input[(window_size / 2) - 1 - 2];
        let mut iterations = 0;
        while pos > window_size16 && iterations < 5000 {
            assert_eq!(input[pos as usize & window_size - 1], end_byte);

            pos = hash_table.get_prev(pos as usize);
            iterations += 1;
        }
    }
}
