const BLOCK_SIZE: u16 = 32000;

const STORED_FIRST_BYTE: u8 = 0b0000_0000;
const STORED_FIRST_BYTE_FINAL: u8 = 0b0000_0001;

/// Split an u16 value into two bytes taking into account endianess
pub fn put16(value: u16) -> (u8, u8) {
    let value = u16::from_le(value);
    let low = value as u8;
    let high = (value >> 8) as u8;
    (low, high)
}

// Compress one stored block
pub fn compress_block_stored(input: &[u8], final_block: bool) -> Vec<u8> {
    // First bit tells us if this is the final chunk
    let first_byte = if final_block {
        STORED_FIRST_BYTE_FINAL
    } else {
        STORED_FIRST_BYTE
    };

    println!("Chunk length: {}", input.len());

    // the next two details compression type (none in this case)
    let (len_0, len_1) = put16(input.len() as u16);
    // the next two after the length is the ones complement of the length
    let (not_len_0, not_len_1) = put16(!input.len() as u16);
    let mut output = vec![first_byte, len_0, len_1, not_len_0, not_len_1];
    output.extend_from_slice(input);
    output
}

pub fn compress_data_stored(input: &[u8]) -> Vec<u8> {
    // TODO: Validate that block size is not too large
    let block_length = BLOCK_SIZE as usize;

    let mut output = Vec::new();
    let mut i = input.chunks(block_length).peekable();
    while let Some(chunk) = i.next() {
        let last_chunk = i.peek().is_none();
        output.extend(compress_block_stored(chunk, last_chunk));
    }
    output
}

#[cfg(test)]
mod test {

    use super::*;

    fn from_bytes(low: u8, high: u8) -> u16 {
        (low as u16) | ((high as u16) << 8)
    }

    #[test]
    fn test_bits() {
        let len = 520u16;
        let (low, high) = put16(len);
        assert_eq!(low, 8);
        assert_eq!(high, 2);

        let test2 = from_bytes(low, high);
        assert_eq!(len, test2);
    }

}
