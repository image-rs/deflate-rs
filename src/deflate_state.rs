use std::io::Write;
use lz77::LZ77State;
use output_writer::DynamicWriter;
use encoder_state::EncoderState;
use input_buffer::{InputBuffer, BackBuffer};
use compression_options::CompressionOptions;
use huffman_table::HuffmanTable;
use std::io;
pub use huffman_table::MAX_MATCH;

pub struct DeflateState<W: Write> {
    pub lz77_state: LZ77State,
    pub input_buffer: InputBuffer,
    pub compression_options: CompressionOptions,
    pub encoder_state: EncoderState<W>,
    pub lz77_writer: DynamicWriter,
    pub bytes_written: u64,
    pub back_buffer: BackBuffer,
}

impl<W: Write> DeflateState<W> {
    pub fn new(compression_options: CompressionOptions, writer: W) -> DeflateState<W> {
        DeflateState {
            input_buffer: InputBuffer::empty(),
            lz77_state: LZ77State::new(compression_options.max_hash_checks,
                                       compression_options.lazy_if_less_than,
                                       compression_options.matching_type),
            encoder_state: EncoderState::new(HuffmanTable::empty(), writer),
            lz77_writer: DynamicWriter::new(),
            compression_options: compression_options,
            bytes_written: 0,
            back_buffer: BackBuffer::new(),
        }
    }

    /// Resets the status of the decoder, leaving the compression options intact
    ///
    /// If flushing the current writer succeeds, it is replaced with the provided one,
    /// buffers and status (except compression options) is reset and the old writer
    /// is returned.
    ///
    /// If flushing fails, the rest of the writer is not cleared.
    pub fn reset(&mut self, writer: W) -> io::Result<W> {
        let ret = self.encoder_state.reset(writer)?;
        self.input_buffer = InputBuffer::empty();
        self.lz77_writer.clear();
        self.lz77_state.reset();
        self.bytes_written = 0;
        self.back_buffer.clear();
        Ok(ret)
    }
}
