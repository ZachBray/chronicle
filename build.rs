extern crate capnpc;

fn main() {
    ::capnpc::compile("src", &["src/schemas/peer_protocol.capnp"]).unwrap();
}
