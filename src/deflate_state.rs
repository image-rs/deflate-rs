use std::io::Write;
use lz77::LZ77State;
use output_writer::DynamicWriter;
use encoder_state::EncoderState;
use input_buffer::InputBuffer;
// use checksum::RollingChecksum;
use compression_options::CompressionOptions;
pub use huffman_table::MAX_MATCH;

pub struct DeflateState<W: Write> {
    pub lz77_state: LZ77State,
    pub input_buffer: InputBuffer,
    pub compression_options: CompressionOptions,
    pub encoder_state: EncoderState<W>,
    pub lz77_writer: DynamicWriter,
}
