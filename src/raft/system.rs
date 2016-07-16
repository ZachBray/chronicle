use time::Duration;

use super::node::Node;
use super::peer::Peer;
use super::log::Log;
use super::messages::Envelope;
use super::network::Network;
use super::configuration::Configuration;

pub struct System<'a, L, N>
    where L: Log,
          N: Network
{
    node: Node<'a, L, N>,
    peers: Vec<Peer>,
}

impl<'a, L, N> System<'a, L, N>
    where L: Log,
          N: Network
{
    pub fn new(log: L, network: N, config: Configuration<'a>) -> Self {
        System {
            peers: config.peer_ids.iter().map(|peer_id| Peer::new(*peer_id)).collect(),
            node: Node::new(log, network, config),
        }
    }

    pub fn run(&mut self, time: Duration) {
        self.node.run(&mut self.peers, time)
    }

    pub fn handle_message(&mut self, envelope: &Envelope) {
        self.node.handle_message(&mut self.peers, envelope);
    }
}
