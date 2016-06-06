# deflate-rs
An attempt at implementing a DEFLATE encoder in rust

So far only uncompressed blocks and blocks using basic lz77 encoding (no lazy matching) and the fixed huffman tables have been implemented. The encoding may not be robust or optimal yet, and is certainly not very optimised.

# License
Both MIT and Apache 2.0
