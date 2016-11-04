use std::cmp;

use chained_hash_table::WINDOW_SIZE;
use huffman_table;

const MAX_MATCH: usize = huffman_table::MAX_MATCH as usize;
const BUFFER_SIZE: usize = (WINDOW_SIZE * 2) + MAX_MATCH;

pub struct InputBuffer {
    buffer: [u8; BUFFER_SIZE],
    //    current_pos: usize,
    current_end: usize,
}

impl InputBuffer {
    pub fn new<'a>(data: &'a [u8]) -> (InputBuffer, Option<&[u8]>) {
        let mut b = InputBuffer {
            buffer: [0; BUFFER_SIZE],
            current_end: cmp::min(data.len(), BUFFER_SIZE), //            current_pos: 0,
        };
        let remaining = init_buffer_from_data(data, &mut b.buffer);
        let input_left = if remaining == 0 {
            None
        } else {
            Some(&data[data.len() - remaining..])
        };
        (b, input_left)
    }

    /// Add
    pub fn add_data<'a>(&'a mut self, data: &'a [u8]) -> Option<&[u8]> {
        if self.current_end + data.len() > self.buffer.len() {
            let len = {
                let mut remaining_buffer = &mut self.buffer[self.current_end..];
                let len = remaining_buffer.len();
                remaining_buffer.copy_from_slice(&data[..len]);
                len
            };
            Some(&data[len..])
        } else {
            self.buffer[self.current_end..self.current_end + data.len()].copy_from_slice(data);
            self.current_end += data.len();
            None
        }
    }

    pub fn slide(&mut self, data: &[u8]) -> usize {
        let (lower, upper) = self.buffer[..].split_at_mut(WINDOW_SIZE);
        lower.copy_from_slice(&upper[..WINDOW_SIZE]);
        let upper_len = upper.len();
        upper[..cmp::min(data.len(), upper_len)].copy_from_slice(data);
        self.current_end = lower.len() + data.len();
        data.len().saturating_sub(upper.len())
    }

    pub fn get_buffer(&mut self) -> &mut [u8] {
        &mut self.buffer[..self.current_end]
    }
}

fn init_buffer_from_data(data: &[u8], buffer: &mut [u8]) -> usize {
    let end = cmp::min(BUFFER_SIZE, data.len());
    buffer[..end].copy_from_slice(&data[..end]);
    data.len().saturating_sub(BUFFER_SIZE)
}

#[cfg(test)]
mod test {
    use super::BUFFER_SIZE;
    use super::*;
    #[test]
    pub fn test_buffer_add_full() {
        let data = [10u8; BUFFER_SIZE + 10];
        let (mut buf, extra) = InputBuffer::new(&data[..]);
        assert_eq!(extra.unwrap(), &[10; 10]);
        let to_add = [2, 5, 3];
        let not_added = buf.add_data(&to_add);
        assert_eq!(not_added.unwrap(), to_add);
    }

    #[test]
    pub fn test_buffer_add_not_full() {
        let data = [10u8; BUFFER_SIZE - 5];
        let (mut buf, extra) = InputBuffer::new(&data[..]);
        assert_eq!(extra, None);
        let to_add = [2, 5, 3];
        {
            let not_added = buf.add_data(&to_add);
            assert!(not_added.is_none());
        }
        let not_added = buf.add_data(&to_add);
        assert_eq!(not_added.unwrap()[0], 3);
    }
}
