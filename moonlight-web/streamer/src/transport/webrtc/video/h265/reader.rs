use std::{
    io::{self, Read},
    ops::Range,
};

use bytes::BytesMut;
use num::FromPrimitive;
use num_derive::FromPrimitive;

use crate::transport::webrtc::video::annexb::{AnnexBSplitter, AnnexBStartCode};

#[allow(unused)]
pub struct Nal {
    pub payload_range: Range<usize>,
    pub header: NalHeader,
    pub header_range: Range<usize>,
    pub start_code: AnnexBStartCode,
    pub start_code_range: Range<usize>,
    pub full: BytesMut,
}

#[derive(Debug, Clone, Copy)]
pub struct NalHeader {
    pub forbidden_zero_bit: bool,
    pub nal_unit_type: NalUnitType,
    pub nuh_layer_id: u8,
    pub nuh_temporal_id_plus1: u8,
}

impl NalHeader {
    pub const SIZE: usize = 2;

    pub fn parse(header: [u8; 2]) -> Self {
        // F: 1 bit
        let forbidden_zero_bit = (header[0] & 0b1000_0000) != 0;

        // Type: 6 bits
        let nal_unit_type = (header[0] & 0b0111_1110) >> 1;

        // LayerId: 6 bits
        let nuh_layer_id = ((header[0] & 0b0000_0001) << 5) | ((header[1] & 0b1111_1000) >> 3);

        // TID: 3 bits
        let nuh_temporal_id_plus1 = header[1] & 0b0000_0111;

        Self {
            forbidden_zero_bit,
            // It's impossible for this to fail because we only have 6 bits like the enum
            #[allow(clippy::unwrap_used)]
            nal_unit_type: NalUnitType::from_u8(nal_unit_type).unwrap(),
            nuh_layer_id,
            nuh_temporal_id_plus1,
        }
    }

    #[allow(unused)]
    pub fn serialize(&self) -> [u8; 2] {
        let mut header = [0u8; 2];

        if self.forbidden_zero_bit {
            header[0] |= 0b1000_0000;
        }

        // Type: 6 bits
        header[0] |= (self.nal_unit_type as u8 & 0b0011_1111) << 1;

        // LayerId: 6 bits
        header[0] |= (self.nuh_layer_id >> 5) & 0b0000_0001;
        header[1] |= (self.nuh_layer_id & 0b0001_1111) << 3;

        // TID: 3 bits
        header[1] |= self.nuh_temporal_id_plus1 & 0b0000_0111;

        header
    }
}

/// Section 7.4.2 in HEVC/H.265 specification (Table 7-1).
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, FromPrimitive)]
pub enum NalUnitType {
    // VCL NAL units
    TrailN = 0,
    TrailR = 1,
    TsaN = 2,
    TsaR = 3,
    StsaN = 4,
    StsaR = 5,
    RadlN = 6,
    RadlR = 7,
    RaslN = 8,
    RaslR = 9,

    RsvVclN10 = 10,
    RsvVclR11 = 11,
    RsvVclN12 = 12,
    RsvVclR13 = 13,
    RsvVclN14 = 14,
    RsvVclR15 = 15,

    BlaWLp = 16,
    BlaWRadl = 17,
    BlaNLp = 18,
    IdrWRadl = 19,
    IdrNLp = 20,
    CraNut = 21,

    RsvIrapVcl22 = 22,
    RsvIrapVcl23 = 23,

    RsvVcl24 = 24,
    RsvVcl25 = 25,
    RsvVcl26 = 26,
    RsvVcl27 = 27,
    RsvVcl28 = 28,
    RsvVcl29 = 29,
    RsvVcl30 = 30,
    RsvVcl31 = 31,

    // Non-VCL NAL units
    VpsNut = 32,
    SpsNut = 33,
    PpsNut = 34,
    AudNut = 35,
    EosNut = 36,
    EobNut = 37,
    FdNut = 38,
    PrefixSeiNut = 39,
    SuffixSeiNut = 40,

    RsvNvcl41 = 41,
    RsvNvcl42 = 42,
    RsvNvcl43 = 43,
    RsvNvcl44 = 44,
    RsvNvcl45 = 45,
    RsvNvcl46 = 46,
    RsvNvcl47 = 47,

    AggregationUnit = 48,
    FragmentationUnit = 49,
    Unspec50 = 50,
    Unspec51 = 51,
    Unspec52 = 52,
    Unspec53 = 53,
    Unspec54 = 54,
    Unspec55 = 55,
    Unspec56 = 56,
    Unspec57 = 57,
    Unspec58 = 58,
    Unspec59 = 59,
    Unspec60 = 60,
    Unspec61 = 61,
    Unspec62 = 62,
    Unspec63 = 63,
}

pub struct H265Reader<R: Read> {
    annex_b: AnnexBSplitter<R>,
}

impl<R> H265Reader<R>
where
    R: Read,
{
    pub fn new(reader: R, capacity: usize) -> Self {
        Self {
            annex_b: AnnexBSplitter::new(reader, capacity),
        }
    }

    /// Read the next NAL unit from the Annex-B stream..
    /// The BytesMut contains the annex-b start code
    pub fn next_nal(&mut self) -> Result<Option<Nal>, io::Error> {
        if let Some(annex_b) = self.annex_b.next()? {
            let header_range = annex_b.payload_range.start..(annex_b.payload_range.start + 2);

            let mut header = [0u8; 2];
            header.copy_from_slice(&annex_b.full[header_range.clone()]);
            let header = NalHeader::parse(header);

            let payload_range = header_range.end..annex_b.payload_range.end;

            Ok(Some(Nal {
                payload_range,
                header,
                header_range,
                start_code: annex_b.start_code,
                start_code_range: annex_b.start_code_range,
                full: annex_b.full,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn reset(&mut self, new_reader: R) {
        self.annex_b.reset(new_reader);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_forbidden_zero_bit() {
        let header_bytes = [0b10000000, 0b00000000]; // forbidden_zero_bit = 1
        let nal = NalHeader::parse(header_bytes);
        assert!(nal.forbidden_zero_bit);

        let header_bytes = [0b00000000, 0b00000000]; // forbidden_zero_bit = 0
        let nal = NalHeader::parse(header_bytes);
        assert!(!nal.forbidden_zero_bit);
    }

    #[test]
    fn test_parse_nal_unit_type() {
        for i in 0..64 {
            let header_bytes = [(i << 1), 0];
            let nal = NalHeader::parse(header_bytes);
            assert_eq!(nal.nal_unit_type as u8, i);
        }
    }

    #[test]
    fn test_parse_layer_id_and_tid() {
        let header_bytes = [0b10000010, 0b10101100];
        let nal = NalHeader::parse(header_bytes);
        let expected_layer_id =
            ((header_bytes[0] & 0b0000_0001) << 5) | ((header_bytes[1] & 0b1111_1000) >> 3);
        assert_eq!(nal.nuh_layer_id, expected_layer_id);
        let expected_tid = header_bytes[1] & 0b0000_0111;
        assert_eq!(nal.nuh_temporal_id_plus1, expected_tid);
    }

    #[test]
    fn test_serialize_round_trip() {
        let original = NalHeader {
            forbidden_zero_bit: true,
            nal_unit_type: NalUnitType::TrailN, // example variant
            nuh_layer_id: 0x2A,
            nuh_temporal_id_plus1: 0x05,
        };
        let serialized = original.serialize();
        let parsed = NalHeader::parse(serialized);
        assert_eq!(parsed.forbidden_zero_bit, original.forbidden_zero_bit);
        assert_eq!(parsed.nal_unit_type as u8, original.nal_unit_type as u8);
        assert_eq!(parsed.nuh_layer_id, original.nuh_layer_id);
        assert_eq!(parsed.nuh_temporal_id_plus1, original.nuh_temporal_id_plus1);
    }

    #[test]
    fn test_serialize_known_values() {
        let nal = NalHeader {
            forbidden_zero_bit: false,
            nal_unit_type: NalUnitType::TrailR,
            nuh_layer_id: 0b010101,
            nuh_temporal_id_plus1: 0b011,
        };
        let bytes = nal.serialize();
        let parsed = NalHeader::parse(bytes);
        assert_eq!(parsed.forbidden_zero_bit, nal.forbidden_zero_bit);
        assert_eq!(parsed.nal_unit_type as u8, nal.nal_unit_type as u8);
        assert_eq!(parsed.nuh_layer_id, nal.nuh_layer_id);
        assert_eq!(parsed.nuh_temporal_id_plus1, nal.nuh_temporal_id_plus1);
    }
}
