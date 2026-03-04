use std::ops::Range;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum Utf8Error {
    #[error("buffer doesn't contain valid utf8 chunks")]
    InvalidChunks,
    #[error("buffer is too small")]
    BufferTooSmall,
}

pub struct ByteBuffer<T> {
    position: usize,
    limit: usize,
    little_endian: bool,
    buffer: T,
}

impl<T> ByteBuffer<T> {
    /// IMPORTANT: no limit checks are made
    /// The limit is used, if this buffer was written before use flip to set it into read mode
    pub fn into_raw(self) -> (T, Range<usize>) {
        (self.buffer, 0..self.limit)
    }
}

#[allow(unused)]
impl<T> ByteBuffer<T>
where
    T: AsRef<[u8]>,
{
    pub fn new(buffer: T) -> Self {
        Self {
            position: 0,
            limit: buffer.as_ref().len(),
            little_endian: false,
            buffer,
        }
    }

    pub fn get_u8_array(&mut self, array: &mut [u8]) {
        array.copy_from_slice(&self.buffer.as_ref()[self.position..(self.position + array.len())]);
        self.position += array.len();
    }
    pub fn get_u8(&mut self) -> u8 {
        let mut buffer = [0u8; 1];
        self.get_u8_array(&mut buffer);
        buffer[0]
    }
    pub fn get_bool(&mut self) -> bool {
        self.get_u8() != 0
    }

    pub fn get_i8(&mut self) -> i8 {
        let byte = self.get_u8();
        if self.little_endian {
            i8::from_le_bytes([byte])
        } else {
            i8::from_be_bytes([byte])
        }
    }

    pub fn get_u16(&mut self) -> u16 {
        let mut buffer = [0u8; 2];
        self.get_u8_array(&mut buffer);

        if self.little_endian {
            u16::from_le_bytes(buffer)
        } else {
            u16::from_be_bytes(buffer)
        }
    }
    pub fn get_i16(&mut self) -> i16 {
        let mut buffer = [0u8; 2];
        self.get_u8_array(&mut buffer);

        if self.little_endian {
            i16::from_le_bytes(buffer)
        } else {
            i16::from_be_bytes(buffer)
        }
    }

    pub fn get_utf8_raw(&mut self, characters: usize) -> Result<&str, Utf8Error> {
        if characters == 0 {
            return Ok("");
        }

        let Some(chunk) = &self.buffer.as_ref()[self.position..].utf8_chunks().next() else {
            return Err(Utf8Error::InvalidChunks);
        };
        let Some((end_char_index, end_char)) = chunk.valid().char_indices().nth(characters - 1)
        else {
            return Err(Utf8Error::BufferTooSmall);
        };
        let output = &chunk.valid()[0..end_char_index + (end_char.len_utf8())];

        Ok(output)
    }

    pub fn get_u32(&mut self) -> u32 {
        let mut buffer = [0u8; 4];
        self.get_u8_array(&mut buffer);

        if self.little_endian {
            u32::from_le_bytes(buffer)
        } else {
            u32::from_be_bytes(buffer)
        }
    }

    pub fn get_f32(&mut self) -> f32 {
        let mut buffer = [0u8; 4];
        self.get_u8_array(&mut buffer);

        if self.little_endian {
            f32::from_le_bytes(buffer)
        } else {
            f32::from_be_bytes(buffer)
        }
    }

    pub fn reset(&mut self) {
        self.position = 0;
        self.limit = 0;
    }
    pub fn flip(&mut self) {
        self.limit = self.position;
        self.position = 0;
    }
    pub fn remaining(&self) -> usize {
        self.limit - self.position
    }
}

#[allow(unused)]
impl<T> ByteBuffer<T>
where
    T: AsMut<[u8]>,
{
    pub fn put_u8_array(&mut self, array: &[u8]) -> bool {
        if self.buffer.as_mut().len() - self.position < array.len() {
            return false;
        }
        self.buffer.as_mut()[self.position..(self.position + array.len())].copy_from_slice(array);

        self.position += array.len();

        true
    }
    pub fn put_u8(&mut self, data: u8) -> bool {
        self.put_u8_array(&[data])
    }
    pub fn put_u16(&mut self, data: u16) -> bool {
        let bytes: [u8; 2] = if self.little_endian {
            u16::to_le_bytes(data)
        } else {
            u16::to_be_bytes(data)
        };

        self.put_u8_array(&bytes)
    }
    pub fn put_u32(&mut self, data: u32) -> bool {
        let bytes: [u8; 4] = if self.little_endian {
            u32::to_le_bytes(data)
        } else {
            u32::to_be_bytes(data)
        };

        self.put_u8_array(&bytes)
    }

    pub fn put_utf8_raw(&mut self, text: &str) -> bool {
        self.put_u8_array(text.as_bytes())
    }
}
