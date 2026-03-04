use bytes::{BufMut, Bytes, BytesMut};
use num::FromPrimitive;
use webrtc::rtp::{self, packetizer::Payloader};

use crate::transport::webrtc::video::h264::{NalHeader, NalUnitType};

#[derive(Debug, Clone, Default)]
pub struct H264Payloader {
    sps_nalu: Option<Bytes>,
    pps_nalu: Option<Bytes>,
}

#[allow(unused)]
#[derive(Debug, Clone, Copy)]
struct FuHeader {
    pub start: bool,
    pub end: bool,
    pub reserved_bit: bool, // always 0
    pub nal_unit_type: NalUnitType,
}

impl FuHeader {
    const SIZE: usize = 1;

    #[allow(unused)]
    pub fn parse(header: [u8; 1]) -> Self {
        let start_bit = (header[0] & 0b1000_0000) != 0;
        let end_bit = (header[0] & 0b0100_0000) != 0;
        let reserved_bit = (header[0] & 0b0010_0000) != 0;
        let nal_unit_type = header[0] & 0b0001_1111;

        Self {
            start: start_bit,
            end: end_bit,
            reserved_bit,
            #[allow(clippy::unwrap_used)]
            nal_unit_type: NalUnitType::from_u8(nal_unit_type).unwrap(),
        }
    }

    pub fn serialize(&self) -> [u8; 1] {
        let mut header = [0u8; 1];

        if self.start {
            header[0] |= 0b1000_0000;
        }
        if self.end {
            header[0] |= 0b0100_0000;
        }
        if self.reserved_bit {
            header[0] |= 0b0010_0000;
        }

        header[0] |= self.nal_unit_type as u8 & 0b0001_1111;

        header
    }
}

// https://datatracker.ietf.org/doc/html/rfc3984#section-5.6
// We're using Non-Interleaved:
// - Only single NAL unit packets, STAP-As, and FU-As MAY be used in this mode.
// - STAP-Bs, MTAPs, and FU-Bs MUST NOT be used.
impl H264Payloader {
    fn build_single_nal(nalu: Bytes) -> Bytes {
        nalu
    }

    // https://datatracker.ietf.org/doc/html/rfc3984#section-5.7.1
    fn build_stap_a_packet(nalus: &[&Bytes]) -> BytesMut {
        let mut stap_len = 1;
        let mut nal_ref_idc = 0;
        for nalu in nalus {
            let nal_header = NalHeader::parse([nalu[0]]);

            if nal_header.nal_ref_idc > nal_ref_idc {
                nal_ref_idc = nal_header.nal_ref_idc;
            }

            stap_len += 2 + nalu.len();
        }

        let stap_a_header = NalHeader {
            forbidden_zero_bit: false,
            nal_ref_idc,
            nal_unit_type: NalUnitType::StapA,
        };

        let mut bytes = BytesMut::with_capacity(stap_len);

        bytes.extend_from_slice(&stap_a_header.serialize());
        for nalu in nalus {
            bytes.put_u16(nalu.len() as u16);
            bytes.extend_from_slice(nalu);
        }

        bytes
    }

    fn build_fragmented_packets(nalu: &Bytes, mut nal_header: NalHeader, mtu: usize) -> Vec<Bytes> {
        let nal_unit_type = nal_header.nal_unit_type;
        nal_header.nal_unit_type = NalUnitType::FragmentationUnit;
        let nal_header = nal_header.serialize();

        let nal_payload = &nalu[1..];

        let mut nal_fragments = nal_payload
            .chunks(mtu - NalHeader::SIZE - FuHeader::SIZE)
            .peekable();

        let mut packets = Vec::with_capacity(nalu.len() / mtu);
        let mut is_first = true;
        while let Some(nal_fragment) = nal_fragments.next() {
            let mut fu_header = FuHeader {
                start: false,
                end: false,
                reserved_bit: false,
                nal_unit_type,
            };

            if is_first {
                fu_header.start = true;
                is_first = false;
            }
            if nal_fragments.peek().is_none() {
                fu_header.end = true;
            }

            let mut packet =
                BytesMut::with_capacity(NalHeader::SIZE + FuHeader::SIZE + nal_fragment.len());

            packet.extend_from_slice(nal_header.as_slice());
            packet.extend_from_slice(&fu_header.serialize());
            packet.extend_from_slice(nal_fragment);

            packets.push(packet.freeze());
        }

        packets
    }
}

impl Payloader for H264Payloader {
    fn payload(&mut self, mtu: usize, b: &Bytes) -> Result<Vec<Bytes>, rtp::Error> {
        if b.len() < NalHeader::SIZE {
            return Err(rtp::Error::ErrBufferTooSmall);
        }

        let nal_header = NalHeader::parse([b[0]]);

        match nal_header.nal_unit_type {
            NalUnitType::AccessUnitDelimiter | NalUnitType::FillerData => {
                return Ok(vec![]);
            }
            NalUnitType::Pps => {
                self.pps_nalu = Some(b.clone());
                return Ok(vec![]);
            }
            NalUnitType::Sps => {
                self.sps_nalu = Some(b.clone());
                return Ok(vec![]);
            }
            _ => {}
        }

        let mut packets = vec![];

        if let (Some(pps), Some(sps)) = (self.pps_nalu.as_ref(), self.sps_nalu.as_ref()) {
            let stap_a = Self::build_stap_a_packet(&[sps, pps]);

            if stap_a.len() >= mtu {
                return Ok(vec![]);
            }

            self.pps_nalu.take();
            self.sps_nalu.take();

            packets.push(stap_a.freeze());
        }

        if b.len() <= mtu {
            packets.push(Self::build_single_nal(b.clone()));
        } else {
            packets.extend(Self::build_fragmented_packets(b, nal_header, mtu));
        }

        Ok(packets)
    }

    fn clone_to(&self) -> Box<dyn Payloader + Send + Sync> {
        Box::new(self.clone())
    }
}
