use std::io::Write;
use lz77::LZ77State;
use output_writer::DynamicWriter;
use encoder_state::EncoderState;
use input_buffer::InputBuffer;
use compression_options::CompressionOptions;
use huffman_table::HuffmanTable;
pub use huffman_table::MAX_MATCH;

pub struct DeflateState<W: Write> {
    pub lz77_state: LZ77State,
    pub input_buffer: InputBuffer,
    pub compression_options: CompressionOptions,
    pub encoder_state: EncoderState<W>,
    pub lz77_writer: DynamicWriter, //    pub checksum: RC,
    pub bytes_written: usize,
}

impl<W: Write> DeflateState<W> {
    pub fn new(input: &[u8],
               compression_options: CompressionOptions,
               writer: W)
               -> DeflateState<W> {
        DeflateState {
            input_buffer: InputBuffer::empty(),
            lz77_state: LZ77State::new(input, compression_options.max_hash_checks),
            encoder_state: EncoderState::new(HuffmanTable::empty(), writer),
            lz77_writer: DynamicWriter::new(),
            compression_options: compression_options,
            bytes_written: 0,
        }
    }
}
