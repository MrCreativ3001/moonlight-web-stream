use bytes::BufMut;
use webrtc::{
    rtp::Error,
    util::{self, Marshal, MarshalSize},
};

pub const COLOR_SPACE_URI: &str = "http://www.webrtc.org/experiments/rtp-hdrext/color-space";

pub const COLOR_SPACE_EXTENSION_SIZE: usize = 4;
pub const HDR_COLOR_SPACE_EXTENSION_SIZE: usize = 28;

#[derive(PartialEq, Eq, Debug, Default, Clone)]
pub struct ColorSpaceExtension {
    pub primaries: u8,
    pub transfer: u8,
    pub matrix: u8,
    pub range_chroma_siting: u8,

    pub hdr_metadata: Option<HdrMetadata>,
}

#[derive(PartialEq, Eq, Debug, Default, Copy, Clone)]
pub struct HdrMetadata {
    pub luminance_max: u16,
    pub luminance_min: u16,
    pub primary_r: (u16, u16),   // x, y
    pub primary_g: (u16, u16),   // x, y
    pub primary_b: (u16, u16),   // x, y
    pub white_point: (u16, u16), // x, y
    pub max_content_light_level: u16,
    pub max_frame_average_light_level: u16,
}

impl MarshalSize for ColorSpaceExtension {
    fn marshal_size(&self) -> usize {
        if self.hdr_metadata.is_some() {
            HDR_COLOR_SPACE_EXTENSION_SIZE
        } else {
            COLOR_SPACE_EXTENSION_SIZE
        }
    }
}

impl Marshal for ColorSpaceExtension {
    fn marshal_to(&self, mut buf: &mut [u8]) -> util::Result<usize> {
        let needed = self.marshal_size();
        if buf.remaining_mut() < needed {
            return Err(Error::ErrBufferTooSmall.into());
        }

        // 1. Basis-Daten schreiben (Big Endian implizit über Byte-Reihenfolge)
        buf.put_u8(self.primaries);
        buf.put_u8(self.transfer);
        buf.put_u8(self.matrix);
        buf.put_u8(self.range_chroma_siting);

        // 2. Optionale HDR-Metadaten schreiben
        if let Some(hdr) = &self.hdr_metadata {
            buf.put_u16(hdr.luminance_max);
            buf.put_u16(hdr.luminance_min);

            buf.put_u16(hdr.primary_r.0);
            buf.put_u16(hdr.primary_r.1);

            buf.put_u16(hdr.primary_g.0);
            buf.put_u16(hdr.primary_g.1);

            buf.put_u16(hdr.primary_b.0);
            buf.put_u16(hdr.primary_b.1);

            buf.put_u16(hdr.white_point.0);
            buf.put_u16(hdr.white_point.1);

            buf.put_u16(hdr.max_content_light_level);
            buf.put_u16(hdr.max_frame_average_light_level);
        }

        Ok(needed)
    }
}
