use std::sync::{Arc};
use std::default::Default;
use std::io::{Read, Write};

use febft::bft::error::*;
use febft::bft::communication::serialize::SharedData;

use self::messages_capnp::request::Action as ActionRequest;

pub struct CalcData;

#[derive(Debug,Clone)]
pub enum Action {
    Sqrt,
    MultiplyByTwo,
    NoOp,
}

impl Action {
    fn ref_to_request(&self) -> ActionRequest {
        match self {
            Action::Sqrt => ActionRequest::Sqrt,
            Action::MultiplyByTwo => ActionRequest::MultiplyByTwo,
            Action::NoOp => ActionRequest::NoOp,
        }
    }
}

impl SharedData for CalcData {
    type State = f32;
    type Request = Arc<Action>;
    type Reply = Arc<f32>;

   fn serialize_state<W>(w: W, s: &f32) -> Result<()>
    where
        W: Write
    {
        bincode::serialize_into(w, s)
            .wrapped(ErrorKind::Communication)
    }

    fn deserialize_state<R>(r: R) -> Result<f32>
    where
        R: Read
    {
        bincode::deserialize_from(r)
            .wrapped(ErrorKind::Communication)
    }

    fn serialize_request<W>(w: W, request: &Self::Request) -> Result<()> where W: Write {
        let mut root = capnp::message::Builder::new(capnp::message::HeapAllocator::new());

        let mut rq_msg: messages_capnp::request::Builder = root.init_root();
        
        rq_msg.set_data(request.ref_to_request());

        capnp::serialize::write_message(w, &root)
            .wrapped_msg(ErrorKind::CommunicationSerialize, "Failed to serialize request")
    }

    fn deserialize_request<R>(r: R) -> Result<Self::Request> where R: Read {

        let reader = capnp::serialize::read_message(r, Default::default()).wrapped_msg(ErrorKind::CommunicationSerialize,
        "Failed to read message")?;

        let request_msg : messages_capnp::request::Reader = reader.get_root()
            .wrapped_msg(ErrorKind::CommunicationSerialize, "Failed to read request message")?;

            let _data = match request_msg.get_data().wrapped_msg(ErrorKind::CommunicationSerialize, "Failed to get data")? {
                ActionRequest::Sqrt => Action::Sqrt,
                ActionRequest::MultiplyByTwo => Action::MultiplyByTwo,
                ActionRequest::NoOp => Action::NoOp,
            };
    
            Ok(Arc::new(_data))
        
    }

    fn serialize_reply<W>(w: W, reply: &Self::Reply) -> Result<()> where W: Write {
        let mut root = capnp::message::Builder::new(capnp::message::HeapAllocator::new());

        let mut rq_msg: messages_capnp::reply::Builder = root.init_root();

        rq_msg.set_data(**reply);
      
        capnp::serialize::write_message(w, &root)
            .wrapped_msg(ErrorKind::CommunicationSerialize, "Failed to serialize reply")
    }

    fn deserialize_reply<R>(r: R) -> Result<Self::Reply> where R: Read {

        let reader = capnp::serialize::read_message(r, Default::default()).wrapped_msg(ErrorKind::CommunicationSerialize,
                                                                                       "Failed to read message")?;

        let request_msg : messages_capnp::reply::Reader = reader.get_root()
            .wrapped_msg(ErrorKind::CommunicationSerialize, "Failed to read reply message")?;

        let _data = request_msg.get_data();

        Ok(Arc::new(_data))
    }
}

mod messages_capnp {
    #![allow(unused)]
    include!(concat!(env!("OUT_DIR"), "/src/serialize/messages_capnp.rs"));
}