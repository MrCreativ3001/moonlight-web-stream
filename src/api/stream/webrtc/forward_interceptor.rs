use std::{collections::VecDeque, time::Instant};

use rtc::{
    interceptor::{self, Attribute, Interceptor, StreamInfo, TaggedPacket},
    rtcp::{
        self,
        payload_feedbacks::{
            full_intra_request::FullIntraRequest, picture_loss_indication::PictureLossIndication,
        },
    },
    sansio::Protocol,
};

/// This forwards PLI and FIR rtcp packets to the actual peer
#[derive(Default)]
pub struct RtcpForwarderInterceptor {
    read_queue: VecDeque<TaggedPacket>,
    write_queue: VecDeque<TaggedPacket>,
}

/// Whether an RTCP packet is a request for a keyframe.
#[allow(clippy::borrowed_box)]
fn is_keyframe_request(packet: &Box<dyn rtcp::Packet>) -> bool {
    let payload = packet.as_any();
    payload.is::<PictureLossIndication>() || payload.is::<FullIntraRequest>()
}

impl Protocol<TaggedPacket, TaggedPacket, ()> for RtcpForwarderInterceptor {
    type Rout = TaggedPacket;
    type Wout = TaggedPacket;
    type Eout = ();
    type Error = webrtc::error::Error;
    type Time = Instant;

    fn handle_read(&mut self, mut msg: TaggedPacket) -> Result<(), Self::Error> {
        if let interceptor::Packet::Rtcp(packets) = &msg.message.packet {
            let requests: Vec<Box<dyn rtcp::Packet>> = packets
                .iter()
                .filter(|packet| is_keyframe_request(packet))
                .cloned()
                .collect();
            if requests.is_empty() {
                // Not the application's business, and the interceptors have already acted on it.
                return Ok(());
            }
            msg.message.packet = interceptor::Packet::Rtcp(requests);
            // Inbound RTCP stops at the end of the chain unless something vouches for it.
            msg.message.add(Attribute::DeliverToApplication);
        }
        self.read_queue.push_back(msg);
        Ok(())
    }

    fn poll_read(&mut self) -> Option<Self::Rout> {
        self.read_queue.pop_front()
    }

    fn handle_write(&mut self, msg: TaggedPacket) -> Result<(), Self::Error> {
        self.write_queue.push_back(msg);
        Ok(())
    }

    fn poll_write(&mut self) -> Option<Self::Wout> {
        self.write_queue.pop_front()
    }
}

impl Interceptor for RtcpForwarderInterceptor {
    fn bind_local_stream(&mut self, _info: &StreamInfo) {}
    fn unbind_local_stream(&mut self, _info: &StreamInfo) {}
    fn bind_remote_stream(&mut self, _info: &StreamInfo) {}
    fn unbind_remote_stream(&mut self, _info: &StreamInfo) {}
}
