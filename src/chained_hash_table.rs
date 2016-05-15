pub const WINDOW_SIZE: usize = 32768;
pub const WINDOW_MASK: usize = WINDOW_SIZE - 1;
const HASH_SHIFT: u32 = 5;
const HASH_MASK: u32 = WINDOW_MASK as u32;

fn update_hash(current_hash: u32, to_insert: u8, shift: u32, mask: u32) -> u32 {
    ((current_hash << shift) ^ (to_insert as u32)) & mask
}

pub struct ChainedHashTable {
    // TODO: Explain properly what these are
    current_hash: u32,
    // Starts of hash chains (in prev?)
    head: Vec<u16>,
    // link to previous occurence of this hash value
    prev: Vec<u16>,

    pub data: Vec<u8>,
}

impl ChainedHashTable {
    fn new() -> ChainedHashTable {
        ChainedHashTable {
            current_hash: 0,
            head: vec!(0; WINDOW_SIZE * 2),
            prev: vec!(0; WINDOW_SIZE),
            data: vec![],
        }
    }

    pub fn from_starting_values(v1: u8, v2: u8) -> ChainedHashTable {
        let mut t = ChainedHashTable::new();
        t.add_hash_value(0, v1);
        t.add_hash_value(1, v2);
        t
    }

    pub fn add_hash_value(&mut self, position: usize, value: u8) {
        self.current_hash = update_hash(self.current_hash, value, HASH_SHIFT, HASH_MASK);
        let position = position & WINDOW_MASK;
        self.prev[position] = self.head[self.current_hash as usize];
        self.head[self.current_hash as usize] = position as u16;
        //        println!("Pos: {}", position);
        self.data.push(value);
    }

    pub fn get_head(&self, bytes: usize) -> u16 {
        self.head[bytes]
    }

    pub fn get_prev(&self, bytes: usize) -> u16 {
        self.prev[bytes]
    }

    pub fn current_hash(&self) -> u32 {
        self.current_hash
    }

    fn slide_value(b: usize, bytes: usize) -> u16 {
        if b + bytes > WINDOW_SIZE {
            (b as u16).wrapping_sub(WINDOW_SIZE as u16)
        } else {
            0
        }
    }

    pub fn slide(&mut self, bytes: usize) {
        for b in &mut self.head {
            *b = ChainedHashTable::slide_value(*b as usize, bytes);
        }

        for b in &mut self.prev {
            *b = ChainedHashTable::slide_value(*b as usize, bytes);
        }
    }
}



#[test]
fn test_update_hash() {
    let start = 0;
    let test = update_hash(start, 147, 5, 32767);
    let test2 = update_hash(test, 147, 5, 32767);
    let test3 = update_hash(test2, 147, 5, 32767);
    println!("{:b}, {:b}, {:b}", test, test2, test3);
    // FIXME: Not sure how to test this yet.
    assert!(false);
}

#[test]
fn test_chained_hash_table() {
    // FIXME: Not sure how to test this yet
    //    let t = ChainedHashTable::from_starting_values(1, 2);
    //    assert!(t.get_head(0) != 0, "head: {}, ", t.get_head(0));
    assert!(false);

}
