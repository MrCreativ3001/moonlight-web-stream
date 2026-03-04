use bytes::{Bytes, BytesMut};
use webrtc::rtp::{self, packetizer::Payloader};

use crate::transport::webrtc::video::h265::reader::{NalHeader, NalUnitType};

#[derive(Debug, Clone, Copy)]
pub struct FuHeader {
    start: bool,
    end: bool,
    nal_unit_type: NalUnitType,
}

impl FuHeader {
    pub const SIZE: usize = 1;

    pub fn serialize(&self) -> u8 {
        let mut header = 0;

        if self.start {
            header |= 0b1000_0000;
        }
        if self.end {
            header |= 0b0100_0000;
        }

        header |= (self.nal_unit_type as u8) & 0b0011_1111;

        header
    }
}

#[derive(Debug, Clone, Default)]
pub struct H265Payloader {
    vps_nalu: Option<Bytes>,
    sps_nalu: Option<Bytes>,
    pps_nalu: Option<Bytes>,
}

impl H265Payloader {
    fn build_single_packet(nalu: Bytes) -> Bytes {
        nalu
    }

    fn build_fragmented_packets(mut nal_header: NalHeader, nalu: &Bytes, mtu: usize) -> Vec<Bytes> {
        let nal_unit_type = nal_header.nal_unit_type;
        nal_header.nal_unit_type = NalUnitType::FragmentationUnit;
        let nal_header = nal_header.serialize();

        let nal_payload = &nalu[2..];

        let mut nal_fragments = nal_payload
            .chunks(mtu - NalHeader::SIZE - FuHeader::SIZE)
            .peekable();

        let mut packets = Vec::with_capacity((nal_payload.len() / mtu) + 1);
        let mut is_first = true;
        while let Some(nal_fragment) = nal_fragments.next() {
            let mut fu_header = FuHeader {
                start: false,
                end: false,
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
            packet.extend_from_slice(&[fu_header.serialize()]);
            packet.extend_from_slice(nal_fragment);

            packets.push(packet.freeze());
        }

        packets
    }

    fn build_aggregation_packet(nalus: &[&Bytes], mtu: usize) -> Bytes {
        let mut aggr_nal_header = NalHeader {
            forbidden_zero_bit: false,
            nuh_layer_id: u8::MAX,
            nuh_temporal_id_plus1: u8::MAX,
            nal_unit_type: NalUnitType::AggregationUnit,
        };

        for nalu in nalus {
            let mut nal_header = [0u8; 2];
            nal_header.copy_from_slice(&nalu[0..2]);
            let header = NalHeader::parse(nal_header);

            if header.forbidden_zero_bit {
                aggr_nal_header.forbidden_zero_bit = true;
            }
            if header.nuh_layer_id < aggr_nal_header.nuh_layer_id {
                aggr_nal_header.nuh_layer_id = header.nuh_layer_id;
            }
            if header.nuh_temporal_id_plus1 < aggr_nal_header.nuh_temporal_id_plus1 {
                aggr_nal_header.nuh_temporal_id_plus1 = header.nuh_temporal_id_plus1;
            }
        }

        let mut aggr_packet = BytesMut::with_capacity(mtu);

        let aggr_nal_header = aggr_nal_header.serialize();
        aggr_packet.extend_from_slice(aggr_nal_header.as_slice());

        for nalu in nalus {
            aggr_packet.extend_from_slice(u16::to_be_bytes(nalu.len() as u16).as_slice());
            aggr_packet.extend_from_slice(nalu);
        }

        aggr_packet.freeze()
    }
}

impl Payloader for H265Payloader {
    fn payload(&mut self, mtu: usize, b: &Bytes) -> Result<Vec<Bytes>, rtp::Error> {
        if b.len() < 2 {
            return Err(rtp::Error::ErrBufferTooSmall);
        }

        // Parse header
        let mut header = [0u8; 2];
        header.copy_from_slice(&b[0..2]);
        let header = NalHeader::parse(header);

        if header.nal_unit_type == NalUnitType::VpsNut {
            self.vps_nalu.replace(b.clone());
        } else if header.nal_unit_type == NalUnitType::SpsNut {
            self.sps_nalu.replace(b.clone());
        } else if header.nal_unit_type == NalUnitType::PpsNut {
            self.pps_nalu.replace(b.clone());
        }

        if let (Some(vps_nalu), Some(sps_nalu), Some(pps_nalu)) =
            (&self.vps_nalu, &self.sps_nalu, &self.pps_nalu)
        {
            let packet = Self::build_aggregation_packet(&[vps_nalu, sps_nalu, pps_nalu], mtu);

            if packet.len() <= mtu {
                self.vps_nalu.take();
                self.sps_nalu.take();
                self.pps_nalu.take();

                return Ok(vec![packet]);
            } else {
                let packets = vec![
                    Self::build_single_packet(vps_nalu.clone()),
                    Self::build_single_packet(sps_nalu.clone()),
                    Self::build_single_packet(pps_nalu.clone()),
                ];

                self.vps_nalu.take();
                self.sps_nalu.take();
                self.pps_nalu.take();

                return Ok(packets);
            }
        } else if matches!(
            header.nal_unit_type,
            NalUnitType::VpsNut | NalUnitType::SpsNut | NalUnitType::PpsNut
        ) {
            return Ok(vec![]);
        }

        if b.len() <= mtu {
            Ok(vec![Self::build_single_packet(b.clone())])
        } else {
            Ok(Self::build_fragmented_packets(header, b, mtu))
        }
    }

    fn clone_to(&self) -> Box<dyn Payloader + Send + Sync> {
        Box::new(self.clone())
    }
}
