#![feature(plugin)]
#![plugin(clippy)]
extern crate time;
extern crate itertools;
extern crate rand;
extern crate capnp;

pub mod raft;
pub mod peer_protocol_capnp {
    include!(concat!(env!("OUT_DIR"), "/peer_protocol_capnp.rs"));
}
