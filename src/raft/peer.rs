use super::measures::{NodeId, LogIndex};
use super::alarm::Alarm;

pub struct Peer {
    pub id: NodeId,
    pub vote_granted: bool,
    pub match_index: LogIndex,
    pub next_index: LogIndex,
    pub rpc_alarm: Alarm,
    pub heartbeat_alarm: Alarm,
}

impl Peer {
    pub fn new(id: NodeId) -> Self {
        Peer {
            id: id,
            vote_granted: false,
            match_index: 0,
            next_index: 1,
            rpc_alarm: Alarm::due_now(),
            heartbeat_alarm: Alarm::due_now(),
        }
    }

    pub fn reset(&mut self) {
        self.vote_granted = false;
        self.match_index = 0;
        self.next_index = 1;
        self.rpc_alarm = Alarm::due_now();
        self.heartbeat_alarm = Alarm::due_now();
    }
}
