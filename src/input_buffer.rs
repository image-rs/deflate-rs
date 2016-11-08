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
    #[cfg(test)]
    pub fn new<'a>(data: &'a [u8]) -> (InputBuffer, Option<&[u8]>) {
        let mut b = InputBuffer::empty();
        let rem = b.add_data(data);
        (b, rem)
    }

    pub fn empty() -> InputBuffer {
        InputBuffer {
            buffer: [0; BUFFER_SIZE],
            current_end: 0,
        }
    }

    /// Add data to the buffer
    /// Returns a slice of the data that was not added (including the lookahead if any)
    pub fn add_data<'a>(&mut self, data: &'a [u8]) -> Option<&'a [u8]> {
        if self.current_end + data.len() > self.buffer.len() {
            let len = {
                let mut remaining_buffer = &mut self.buffer[self.current_end..];
                let len = remaining_buffer.len();
                remaining_buffer.copy_from_slice(&data[..len]);
                len
            };
            self.current_end = BUFFER_SIZE;
            Some(&data[len..])
        } else {
            self.buffer[self.current_end..self.current_end + data.len()].copy_from_slice(data);
            self.current_end += data.len();
            None
        }
    }

    pub fn current_end(&self) -> usize {
        self.current_end
    }

    // Slide the input window, add new data and return a slice containing the data that did not fit, if any
    pub fn slide<'a>(&mut self, data: &'a [u8]) -> Option<&'a [u8]> {
        // This should only be used when the buffer is full
        assert!(self.current_end == BUFFER_SIZE);
        // Split into lower window and upper window + lookahead
        let (lower, upper) = self.buffer[..].split_at_mut(WINDOW_SIZE);
        // Copy the upper window to the lower window
        lower.copy_from_slice(&upper[..WINDOW_SIZE]);
        {
            // Copy the lookahead to the start of the upper window
            let (upper_2, lookahead) = upper.split_at_mut(WINDOW_SIZE);
            upper_2[..MAX_MATCH].copy_from_slice(lookahead);
        }

        // Length of the upper window minus the lookahead bytes
        let upper_len = upper.len() - MAX_MATCH;
        let end = cmp::min(data.len(), upper_len);
        upper[MAX_MATCH..MAX_MATCH + end].copy_from_slice(&data[..end]);
        self.current_end = lower.len() + MAX_MATCH + end;

        if data.len() > upper_len {
            // Return a slice of the data that was not added
            Some(&data[end..])
        } else {
            None
        }
    }

    pub fn get_buffer(&mut self) -> &mut [u8] {
        &mut self.buffer[..self.current_end]
    }
}

#[cfg(test)]
mod test {
    use super::BUFFER_SIZE;
    use super::MAX_MATCH;
    use chained_hash_table::WINDOW_SIZE;
    use super::*;
    #[test]
    pub fn buffer_add_full() {
        let data = [10u8; BUFFER_SIZE + 10];
        let (mut buf, extra) = InputBuffer::new(&data[..]);
        assert_eq!(extra.unwrap(), &[10; 10]);
        let to_add = [2, 5, 3];
        let not_added = buf.add_data(&to_add);
        assert_eq!(not_added.unwrap(), to_add);
    }

    #[test]
    pub fn buffer_add_not_full() {
        let data = [10u8; BUFFER_SIZE - 5];
        let (mut buf, extra) = InputBuffer::new(&data[..]);
        assert_eq!(buf.current_end(), data.len());
        assert_eq!(extra, None);
        let to_add = [2, 5, 3];
        {
            let not_added = buf.add_data(&to_add);
            assert!(not_added.is_none());
        }
        let not_added = buf.add_data(&to_add);
        assert_eq!(not_added.unwrap()[0], 3);
    }

    #[test]
    fn slide() {
        let data = [10u8; BUFFER_SIZE];
        let (mut buf, extra) = InputBuffer::new(&data[..]);
        assert_eq!(extra, None);
        let to_add = [5; 5];
        let rem = buf.slide(&to_add);
        assert!(rem.is_none());
        {
            let slice = buf.get_buffer();
            assert!(slice[..WINDOW_SIZE + MAX_MATCH] == data[WINDOW_SIZE..]);
            assert_eq!(slice[WINDOW_SIZE + MAX_MATCH..WINDOW_SIZE + MAX_MATCH + 5], to_add);
        }
        assert_eq!(buf.current_end(), WINDOW_SIZE + MAX_MATCH + to_add.len());
    }
}
