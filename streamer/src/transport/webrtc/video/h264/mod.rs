//! Specifications:
//! - https://datatracker.ietf.org/doc/html/rfc3984

use std::ops::Range;

use bytes::BytesMut;
use num::FromPrimitive;
use num_derive::FromPrimitive;

use crate::transport::webrtc::video::annexb::AnnexBStartCode;

pub mod payloader;

pub mod reader;

#[allow(unused)]
pub struct Nal {
    pub payload_range: Range<usize>,
    pub header: NalHeader,
    pub header_range: Range<usize>,
    pub start_code: AnnexBStartCode,
    pub start_code_range: Range<usize>,
    pub full: BytesMut,
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, FromPrimitive)]
pub enum NalUnitType {
    // VCL NAL units
    Unspecified = 0,
    CodedSliceNonIDR = 1,
    CodedSliceDataPartitionA = 2,
    CodedSliceDataPartitionB = 3,
    CodedSliceDataPartitionC = 4,
    CodedSliceIDR = 5,
    Sei = 6,
    Sps = 7,
    Pps = 8,
    AccessUnitDelimiter = 9,
    EndOfSequence = 10,
    EndOfStream = 11,
    FillerData = 12,
    SPSEx = 13,
    PrefixNALUnit = 14,
    SubsetSPS = 15,
    DepthParameterSet = 16,
    Reserved17 = 17,
    Reserved18 = 18,
    CodedSliceAux = 19,
    CodedSliceExt = 20,
    CodedSliceExtDepth = 21,
    Reserved22 = 22,
    Reserved23 = 23,
    StapA = 24,
    Unspecified25 = 25,
    Unspecified26 = 26,
    Unspecified27 = 27,
    FragmentationUnit = 28,
    Unspecified29 = 29,
    Unspecified30 = 30,
    Unspecified31 = 31,
}

// https://datatracker.ietf.org/doc/html/rfc3984#section-1.3
#[allow(unused)]
#[derive(Debug, Clone, Copy)]
pub struct NalHeader {
    pub forbidden_zero_bit: bool,
    pub nal_ref_idc: u8,
    pub nal_unit_type: NalUnitType,
}

impl NalHeader {
    pub const SIZE: usize = 1;

    pub fn parse(header: [u8; 1]) -> Self {
        // F: 1 bit
        let forbidden_zero_bit = ((header[0] & 0b10000000) >> 7) == 1;

        // NRI: 2 bits
        let nal_ref_idc = (header[0] & 0b01100000) >> 5;

        // Type: 5 bits
        let nal_unit_type = header[0] & 0b00011111;

        Self {
            forbidden_zero_bit,
            nal_ref_idc,
            #[allow(clippy::unwrap_used)]
            nal_unit_type: NalUnitType::from_u8(nal_unit_type).unwrap(),
        }
    }

    pub fn serialize(&self) -> [u8; 1] {
        let mut header = [0u8; 1];

        // F: Forbidden zero bit (bit 7)
        if self.forbidden_zero_bit {
            header[0] |= 0b1000_0000;
        }

        // NRI: 2 bits (bits 6–5)
        header[0] |= (self.nal_ref_idc & 0b11) << 5;

        // Type: 5 bits (bits 4–0)
        header[0] |= (self.nal_unit_type as u8) & 0b0001_1111;

        header
    }
}
