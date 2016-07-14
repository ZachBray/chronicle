use super::messages::Envelope;

pub trait Network {
    fn send(&mut self, envelope: Envelope);
}
