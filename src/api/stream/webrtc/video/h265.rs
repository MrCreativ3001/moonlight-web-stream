use bytes::{Bytes, BytesMut};
use rtc::rtp::packetizer::Payloader;

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
            nal_unit_type: NalUnitType::from_u8(nal_unit_type),
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

impl NalUnitType {
    pub fn from_u8(x: u8) -> Self {
        match x {
            0 => Self::TrailN,
            1 => Self::TrailR,
            2 => Self::TsaN,
            3 => Self::TsaR,
            4 => Self::StsaN,
            5 => Self::StsaR,
            6 => Self::RadlN,
            7 => Self::RadlR,
            8 => Self::RaslN,
            9 => Self::RaslR,
            10 => Self::RsvVclN10,
            11 => Self::RsvVclR11,
            12 => Self::RsvVclN12,
            13 => Self::RsvVclR13,
            14 => Self::RsvVclN14,
            15 => Self::RsvVclR15,
            16 => Self::BlaWLp,
            17 => Self::BlaWRadl,
            18 => Self::BlaNLp,
            19 => Self::IdrWRadl,
            20 => Self::IdrNLp,
            21 => Self::CraNut,
            22 => Self::RsvIrapVcl22,
            23 => Self::RsvIrapVcl23,
            24 => Self::RsvVcl24,
            25 => Self::RsvVcl25,
            26 => Self::RsvVcl26,
            27 => Self::RsvVcl27,
            28 => Self::RsvVcl28,
            29 => Self::RsvVcl29,
            30 => Self::RsvVcl30,
            31 => Self::RsvVcl31,
            32 => Self::VpsNut,
            33 => Self::SpsNut,
            34 => Self::PpsNut,
            35 => Self::AudNut,
            36 => Self::EosNut,
            37 => Self::EobNut,
            38 => Self::FdNut,
            39 => Self::PrefixSeiNut,
            40 => Self::SuffixSeiNut,
            41 => Self::RsvNvcl41,
            42 => Self::RsvNvcl42,
            43 => Self::RsvNvcl43,
            44 => Self::RsvNvcl44,
            45 => Self::RsvNvcl45,
            46 => Self::RsvNvcl46,
            47 => Self::RsvNvcl47,
            48 => Self::AggregationUnit,
            49 => Self::FragmentationUnit,
            50 => Self::Unspec50,
            51 => Self::Unspec51,
            52 => Self::Unspec52,
            53 => Self::Unspec53,
            54 => Self::Unspec54,
            55 => Self::Unspec55,
            56 => Self::Unspec56,
            57 => Self::Unspec57,
            58 => Self::Unspec58,
            59 => Self::Unspec59,
            60 => Self::Unspec60,
            61 => Self::Unspec61,
            62 => Self::Unspec62,
            63 => Self::Unspec63,
            _ => Self::Unspec63,
        }
    }
}

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
    fn payload(&mut self, mtu: usize, b: &Bytes) -> Result<Vec<Bytes>, webrtc::error::Error> {
        if b.len() < 2 {
            return Err(webrtc::error::Error::ErrBufferTooSmall);
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

    fn clone_to(&self) -> Box<dyn Payloader> {
        Box::new(self.clone())
    }
}
