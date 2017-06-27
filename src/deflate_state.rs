use std::io::Write;
use std::{io, mem, cmp};

use lz77::LZ77State;
use output_writer::DynamicWriter;
use encoder_state::EncoderState;
use input_buffer::InputBuffer;
use compression_options::{CompressionOptions, MAX_HASH_CHECKS};
use huffman_table::HuffmanTable;
use compress::Flush;
pub use huffman_table::MAX_MATCH;


/// A struct containing all the stored state used for the encoder.
pub struct DeflateState<W: Write> {
    /// State of lz77 compression.
    pub lz77_state: LZ77State,
    pub input_buffer: InputBuffer,
    pub compression_options: CompressionOptions,
    /// State the huffman part of the compression and the output buffer.
    pub encoder_state: EncoderState,
    /// The buffer containing the raw output of the lz77-encoding.
    pub lz77_writer: DynamicWriter,
    /// Total number of bytes consumed/written to the input buffer.
    pub bytes_written: u64,
    /// Wrapped writer.
    /// Option is used to allow us to implement `Drop` and `finish()` at the same time for the
    /// writer structs.
    pub inner: Option<W>,
    /// The position in the output buffer where data should be flushed from, to keep track of
    /// what data has been output in case not all data is output when writing to the wrapped
    /// writer.
    pub output_buf_pos: usize,
    pub flush_mode: Flush,
    /// Number of bytes written as calculated by sum of block input lengths.
    /// Used to check that they are correct when `debug_assertions` are enabled.
    pub bytes_written_control: u64,
}

impl<W: Write> DeflateState<W> {
    pub fn new(compression_options: CompressionOptions, writer: W) -> DeflateState<W> {
        DeflateState {
            input_buffer: InputBuffer::empty(),
            lz77_state: LZ77State::new(
                compression_options.max_hash_checks,
                cmp::min(compression_options.lazy_if_less_than, MAX_HASH_CHECKS),
                compression_options.matching_type,
            ),
            encoder_state: EncoderState::new(HuffmanTable::empty(), Vec::with_capacity(1024 * 32)),
            lz77_writer: DynamicWriter::new(),
            compression_options: compression_options,
            bytes_written: 0,
            inner: Some(writer),
            output_buf_pos: 0,
            flush_mode: Flush::None,
            bytes_written_control: 0,
        }
    }

    pub fn output_buf(&mut self) -> &mut Vec<u8> {
        self.encoder_state.inner_vec()
    }

    /// Resets the status of the decoder, leaving the compression options intact
    ///
    /// If flushing the current writer succeeds, it is replaced with the provided one,
    /// buffers and status (except compression options) is reset and the old writer
    /// is returned.
    ///
    /// If flushing fails, the rest of the writer is not cleared.
    pub fn reset(&mut self, writer: W) -> io::Result<W> {
        self.encoder_state.flush()?;
        self.inner.as_mut().expect("Missing writer!").write_all(
            self.encoder_state.inner_vec(),
        )?;
        self.encoder_state.inner_vec().clear();
        self.input_buffer = InputBuffer::empty();
        self.lz77_writer.clear();
        self.lz77_state.reset();
        self.bytes_written = 0;
        self.output_buf_pos = 0;
        self.flush_mode = Flush::None;
        if cfg!(debug_assertions) {
            self.bytes_written_control = 0;
        }
        mem::replace(&mut self.inner, Some(writer)).ok_or_else(|| {
            io::Error::new(io::ErrorKind::Other, "Missing writer")
        })
    }
}
