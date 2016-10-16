use std::cmp;

use chained_hash_table::WINDOW_SIZE;
use huffman_table;

const MAX_MATCH: usize = huffman_table::MAX_MATCH as usize;
const BUFFER_SIZE: usize = (WINDOW_SIZE * 2) + MAX_MATCH;

pub struct InputBuffer {
    buffer: [u8; BUFFER_SIZE],
    current_end: usize,
}

impl InputBuffer {
    pub fn new(data: &[u8]) -> InputBuffer {
        let mut b = InputBuffer {
            buffer: [0; BUFFER_SIZE],
            current_end: cmp::min(data.len(), BUFFER_SIZE),
        };
        init_buffer_from_data(data, &mut b.buffer);
        b
    }

    pub fn slide(&mut self, data: &[u8]) {
        let (lower, upper) = self.buffer[..].split_at_mut(WINDOW_SIZE);
        lower.copy_from_slice(&upper[..WINDOW_SIZE]);
        upper[..data.len()].copy_from_slice(data);
        self.current_end = lower.len() + data.len();
    }

    pub fn get_buffer(&mut self) -> &mut [u8] {
        &mut self.buffer[..self.current_end]
    }
}

fn init_buffer_from_data(data: &[u8], buffer: &mut [u8]) {
    let end = cmp::min(BUFFER_SIZE, data.len());
    buffer[..end].copy_from_slice(&data[..end]);
}
