<a name="0.8.5"></a>
### 0.8.5 (2020-04-04)
Fix block size counter bug #44 (probably introduced in 1b70be)
that triggered a debug assertion and that could possibly in theory cause stored block to start at the wrong input position at a block split with low entropy data followed by uncompressible data.
