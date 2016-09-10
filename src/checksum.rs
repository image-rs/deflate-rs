use adler32::RollingAdler32;

pub trait RollingChecksum {
    fn new() -> Self;
    fn update(&mut self, byte: u8);
    fn update_from_slice(&mut self, data: &[u8]);
    fn current_hash(&self) -> u32;
}

pub struct NoChecksum {
}

impl RollingChecksum for NoChecksum {
    fn new() -> NoChecksum {
        NoChecksum {}
    }
    fn update(&mut self, _: u8) {}
    fn update_from_slice(&mut self, _: &[u8]) {}
    fn current_hash(&self) -> u32 {
        1
    }
}

pub struct Adler32Checksum {
    adler32: RollingAdler32,
}

impl RollingChecksum for Adler32Checksum {
    fn new() -> Self {
        Adler32Checksum { adler32: RollingAdler32::new() }
    }
    fn update(&mut self, byte: u8) {
        self.adler32.update(byte);
    }

    fn update_from_slice(&mut self, data: &[u8]) {
        self.adler32.update_buffer(data);
    }

    fn current_hash(&self) -> u32 {
        self.adler32.hash()
    }
}
