use std::io::Write;
use lz77::LZ77State;
use output_writer::DynamicWriter;
use encoder_state::EncoderState;
use input_buffer::InputBuffer;
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
    pub bytes_written: usize,
}

impl<W: Write> DeflateState<W> {
    pub fn _new_with_data(input: &[u8],
                          compression_options: CompressionOptions,
                          writer: W)
                          -> DeflateState<W> {
        DeflateState {
            input_buffer: InputBuffer::empty(),
            lz77_state: LZ77State::_new_warmup(input,
                                               compression_options.max_hash_checks,
                                               compression_options.lazy_if_less_than),
            encoder_state: EncoderState::new(HuffmanTable::empty(), writer),
            lz77_writer: DynamicWriter::new(),
            compression_options: compression_options,
            bytes_written: 0,
        }
    }

    pub fn new(compression_options: CompressionOptions, writer: W) -> DeflateState<W> {
        DeflateState {
            input_buffer: InputBuffer::empty(),
            lz77_state: LZ77State::new(compression_options.max_hash_checks,
                                       compression_options.lazy_if_less_than),
            encoder_state: EncoderState::new(HuffmanTable::empty(), writer),
            lz77_writer: DynamicWriter::new(),
            compression_options: compression_options,
            bytes_written: 0,
        }
    }

    /// Resets the status of the decoder, leaving the compression options intact
    ///
    /// If flushing the current writer succeeds, it is replaced with the provided one,
    /// buffers and status (except compression options) is reset and the old writer
    /// is returned.
    pub fn reset(&mut self, writer: W) -> io::Result<W> {
        let ret = self.encoder_state.reset(writer)?;
        self.input_buffer = InputBuffer::empty();
        self.lz77_writer.clear();
        self.lz77_state.reset();
        self.bytes_written = 0;
        Ok(ret)
    }
}
