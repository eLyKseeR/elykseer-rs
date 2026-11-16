use std::convert::TryFrom;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct Nchunks(u32);

#[derive(Debug, Clone, Copy)]
pub struct NchunkOutOfRange;

impl Nchunks {
    pub const MIN: u32 = 16;
    pub const MAX: u32 = 256;

    pub fn new(value: u32) -> Self {
        let clamped = value.clamp(Self::MIN, Self::MAX);
        Nchunks(clamped)
    }

    pub fn from_positive(value: u32) -> Self {
        Nchunks::new(value)
    }

    pub fn from_int(value: i32) -> Self {
        // Convert int to positive, then clamp to [16, 256]
        let positive = value.try_into().unwrap_or(0);
        Nchunks::new(positive)
    }

    pub fn to_positive(self) -> u32 {
        self.0
    }

    pub fn to_n(self) -> u64 {
        // Assuming Conversion.pos2N is a conversion to N (natural numbers)
        self.0 as u64
    }
}

// Implement TryFrom for u32
impl TryFrom<u32> for Nchunks {
    type Error = NchunkOutOfRange;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        if !(Nchunks::MIN..=Nchunks::MAX).contains(&value) {
            return Err(NchunkOutOfRange);
        }
        Ok(Nchunks(value))
    }
}

// Implement TryFrom for i32
impl TryFrom<i32> for Nchunks {
    type Error = NchunkOutOfRange;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        let positive: u32 = value.try_into().map_err(|_| NchunkOutOfRange)?;
        Nchunks::try_from(positive)
    }
}
