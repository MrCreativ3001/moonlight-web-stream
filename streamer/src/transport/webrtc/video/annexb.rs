use std::{
    io::{self, ErrorKind, Read},
    ops::Range,
};

use bytes::{BufMut, BytesMut};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnnexBStartCode {
    B3,
    B4,
}

impl AnnexBStartCode {
    pub fn code(&self) -> &'static [u8] {
        match self {
            Self::B3 => &[0, 0, 1],
            Self::B4 => &[0, 0, 0, 1],
        }
    }
    pub fn len(&self) -> usize {
        match self {
            Self::B3 => self.code().len(),
            Self::B4 => self.code().len(),
        }
    }
}

pub struct AnnexBData {
    pub payload_range: Range<usize>,
    pub start_code: AnnexBStartCode,
    pub start_code_range: Range<usize>,
    pub full: BytesMut,
}

pub struct AnnexBSplitter<R: Read> {
    reader: R,
    start_code_buffer: [u8; 4],
    current_start_code: Option<AnnexBStartCode>,
    buffer: BytesMut,
}

impl<R> AnnexBSplitter<R>
where
    R: Read,
{
    pub fn new(reader: R, capacity: usize) -> Self {
        Self {
            reader,
            start_code_buffer: [2u8; 4],
            current_start_code: None,
            buffer: BytesMut::with_capacity(capacity),
        }
    }

    pub fn next(&mut self) -> Result<Option<AnnexBData>, io::Error> {
        if self.current_start_code.is_none() {
            if let Some(start_code) = self.next_annex_b_start_code(false)? {
                self.current_start_code = Some(start_code);
                self.reset_start_code_buffer();

                self.buffer.put(start_code.code());
            } else {
                return Ok(None);
            }
        }

        #[allow(clippy::unwrap_used)]
        let current_start_code = self.current_start_code.unwrap();

        let result = self.next_annex_b_start_code(true);

        match result {
            Ok(None) => {
                self.current_start_code = None;

                Ok(Some(AnnexBData {
                    payload_range: current_start_code.len()..self.buffer.len(),
                    start_code: current_start_code,
                    start_code_range: 0..current_start_code.len(),
                    full: self.buffer.split(),
                }))
            }
            Err(err) if err.kind() == ErrorKind::UnexpectedEof => {
                self.current_start_code = None;

                Ok(Some(AnnexBData {
                    payload_range: current_start_code.len()..self.buffer.len(),
                    start_code: current_start_code,
                    start_code_range: 0..current_start_code.len(),
                    full: self.buffer.split(),
                }))
            }
            Ok(Some(next_start_code)) => {
                let mut full = self.buffer.split();
                full.truncate(full.len() - next_start_code.len());

                self.current_start_code = Some(next_start_code);
                self.buffer.put(next_start_code.code());
                self.reset_start_code_buffer();

                Ok(Some(AnnexBData {
                    payload_range: current_start_code.len()..full.len(),
                    start_code: current_start_code,
                    start_code_range: 0..current_start_code.len(),
                    full,
                }))
            }
            Err(err) => Err(err),
        }
    }

    pub fn reset(&mut self, new_reader: R) {
        // Read to end
        while let Ok(Some(_)) = self.next() {}

        self.reader = new_reader;
    }

    fn next_annex_b_start_code(
        &mut self,
        buffer_bytes: bool,
    ) -> Result<Option<AnnexBStartCode>, io::Error> {
        loop {
            match &self.start_code_buffer {
                [0, 0, 0, 1] => return Ok(Some(AnnexBStartCode::B4)),
                [_, 0, 0, 1] => return Ok(Some(AnnexBStartCode::B3)),
                _ => {}
            }

            let mut byte = [0u8; 1];
            if self.reader.read(&mut byte)? == 0 {
                return Ok(None);
            }
            let byte = byte[0];

            if buffer_bytes {
                self.buffer.put_u8(byte);
            }

            self.start_code_buffer.rotate_left(1);
            self.start_code_buffer[3] = byte;
        }
    }
    fn reset_start_code_buffer(&mut self) {
        self.start_code_buffer.copy_from_slice(&[2u8; 4]);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_splitter_single_b3() {
        let data = vec![0, 0, 1, 0x42, 0x01, 0x02];
        let mut splitter = AnnexBSplitter::new(Cursor::new(data.clone()), 16);

        let nal = splitter.next().unwrap().unwrap();
        assert_eq!(nal.start_code, AnnexBStartCode::B3);
        assert_eq!(&nal.full[nal.payload_range.clone()], &[0x42, 0x01, 0x02]);
    }

    #[test]
    fn test_splitter_single_b4() {
        let data = vec![0, 0, 0, 1, 0x44, 0x05];
        let mut splitter = AnnexBSplitter::new(Cursor::new(data.clone()), 16);

        let nal = splitter.next().unwrap().unwrap();
        assert_eq!(nal.start_code, AnnexBStartCode::B4);
        assert_eq!(&nal.full[nal.payload_range.clone()], &[0x44, 0x05]);
    }

    #[test]
    fn test_splitter_multiple_nalus() {
        let data = vec![0, 0, 0, 1, 0x42, 0x01, 0x02, 0, 0, 1, 0x44, 0x03, 0x04];
        let mut splitter = AnnexBSplitter::new(Cursor::new(data.clone()), 32);

        let nal1 = splitter.next().unwrap().unwrap();
        assert_eq!(nal1.start_code, AnnexBStartCode::B4);
        assert_eq!(&nal1.full[nal1.payload_range.clone()], &[0x42, 0x01, 0x02]);

        let nal2 = splitter.next().unwrap().unwrap();
        assert_eq!(nal2.start_code, AnnexBStartCode::B3);
        assert_eq!(&nal2.full[nal2.payload_range.clone()], &[0x44, 0x03, 0x04]);
    }

    #[test]
    fn test_splitter_no_nalus() {
        let data = vec![0x01, 0x02, 0x03];
        let mut splitter = AnnexBSplitter::new(Cursor::new(data.clone()), 16);

        let nal = splitter.next().unwrap();
        assert!(nal.is_none());
    }

    #[test]
    fn test_splitter_edge_case_start_of_stream() {
        let data = vec![0, 0, 0, 1, 0x42];
        let mut splitter = AnnexBSplitter::new(Cursor::new(data.clone()), 16);

        let nal = splitter.next().unwrap().unwrap();
        assert_eq!(nal.start_code, AnnexBStartCode::B4);
        assert_eq!(&nal.full[nal.payload_range.clone()], &[0x42]);
    }
}
