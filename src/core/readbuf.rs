#[derive(Debug)]
pub struct ReadBuffer {
    storage: Vec<u8>,
}

impl ReadBuffer {
    pub fn new() -> Self {
        Self { storage: Vec::with_capacity(16) }
    }
}
