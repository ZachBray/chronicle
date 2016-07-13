extern crate uuid;
extern crate time;
extern crate itertools;
use uuid::Uuid;
use std::vec::Vec;
use std::collections::HashMap;
use time::Duration;
use itertools::Itertools;

type ServerId = Uuid;
type Term = u64;
type LogIndex = u64;

struct LogEntry {
    term: Term,
    data: [u8]
}

enum Message<'a> {
    RequestVoteRequest { 
        last_log_term: Term, 
        last_log_index: LogIndex 
    },

    AppendEntriesRequest {
        prev_index: LogIndex,
        prev_term: Term,
        entries: &'a [&'a LogEntry],
        commit_index: LogIndex
    }
}

struct Envelope<'a> {
    from: ServerId,
    to: ServerId,
    term: Term,
    message: Message<'a>
}

enum ServerState {
    Follower,
    Candidate,
    Leader
}


trait Model {
    fn time(&self) -> Duration;
    fn random_election_timeout(&mut self) -> Duration;
    fn max_election_timeout(&self) -> Duration;
    fn rpc_timeout(&self) -> Duration;
    fn send(&self, envelope: Envelope);
    fn batch_size(&self) -> LogIndex;
}

struct Alarm {
    due_time: Duration
}

impl Alarm {
    fn due_now() -> Self {
        Alarm {
            due_time: Duration::min_value()
        }
    }

    fn due_never() -> Self {
        Alarm {
            due_time: Duration::max_value()
        }
    }

    fn new(due_time: Duration) -> Self {
        Alarm {
            due_time: due_time
        }
    }

    fn is_due<TModel>(&self, model: &TModel) -> bool 
    where TModel: Model {
        self.due_time <= model.time()
    }
}

struct Peer {
    vote_granted: bool,
    match_index: LogIndex,
    next_index: LogIndex,
    rpc_alarm: Alarm,
    heartbeat_alarm: Alarm
}

impl Peer {
    fn reset(&mut self) {
        self.vote_granted = false;
        self.match_index = 0;
        self.next_index = 1;
        self.rpc_alarm = Alarm::due_now();
        self.heartbeat_alarm = Alarm::due_now();
    }
}

trait Log {
    fn term(&self) -> Term {
        self.term_at(self.length())
    }

    fn term_at(&self, index: LogIndex) -> Term;
    fn length(&self) -> LogIndex;
    fn entries<'a>(&'a self, from_index_incl: LogIndex, until_index_excl: LogIndex) -> &'a[&'a LogEntry];
}

struct Server<TLog> where TLog: Log {
    id: ServerId,
    term: Term,
    commit_index: LogIndex,
    state: ServerState,
    voted_for: Option<ServerId>,
    election_alarm: Alarm,
    peers: HashMap<ServerId, Peer>,
    log: TLog
}

impl <TLog> Server<TLog> where TLog: Log {
    fn step_down<TModel>(&mut self, model: &mut TModel, term: u64) where TModel: Model {
        self.term = term; 
        self.state = ServerState::Follower;
        self.voted_for = Option::None;
        if self.election_alarm.is_due(model) {
            self.election_alarm = Alarm::new(model.random_election_timeout());
        }
    }

    fn start_new_election<TModel>(&mut self, model: &mut TModel) where TModel: Model {
        match self.state {
            ServerState::Follower | ServerState::Candidate if self.election_alarm.is_due(model) => {
                self.election_alarm = Alarm::new(model.random_election_timeout());
                self.term += 1;
                self.voted_for = Option::Some(self.id);
                self.state = ServerState::Candidate;
                for (_, peer) in self.peers.iter_mut() {
                    peer.reset();
                }
            },
            _ => {}
        }
    }

    fn send_request_vote<TModel>(&mut self, model: &mut TModel, peer_id: &ServerId) where TModel: Model {
        match self.state {
            ServerState::Candidate => {
                match self.peers.get_mut(peer_id) {
                    Option::Some(peer) => {
                        if peer.rpc_alarm.is_due(model) {
                            peer.rpc_alarm = Alarm::new(model.time() + model.rpc_timeout());
                            model.send(Envelope {
                                from: self.id,
                                to: peer_id.clone(),
                                term: self.term,
                                message: Message::RequestVoteRequest {
                                    last_log_term: self.log.term(),
                                    last_log_index: self.log.length()
                                }
                            });
                        }
                    },
                    _ => {}
                }
            }
            _ => {}
        }
    }

    fn become_leader(&mut self) {
        match self.state {
            ServerState::Candidate => {
                let votes_for_this_server = self.peers.iter().filter(|&(_, p)| p.vote_granted).count();
                let num_servers = self.peers.len();
                let has_quorum = votes_for_this_server + 1 / (num_servers / 2);
                self.state = ServerState::Leader;
                for (_, peer) in self.peers.iter_mut() {
                    peer.next_index = self.log.length() + 1;
                    peer.rpc_alarm = Alarm::due_never();
                    peer.heartbeat_alarm = Alarm::due_now();
                }
                self.election_alarm = Alarm::due_never();
            },
            _ => {}
        }
    }

    fn send_append_entries<TModel>(&mut self, model: &mut TModel, peer_id: &ServerId) where TModel: Model {
        match self.peers.get_mut(peer_id) {
            Option::Some(peer) =>
                match self.state {
                    ServerState::Leader if 
                        peer.heartbeat_alarm.is_due(model)
                        || (peer.rpc_alarm.is_due(model) && peer.next_index <= self.log.length()) => {
                        let prev_index = peer.next_index - 1;
                        let last_index = 
                            if peer.match_index + 1 < peer.next_index {
                                prev_index
                            } else {
                                std::cmp::min(prev_index + model.batch_size(), self.log.length())
                            };
                        model.send(Envelope {
                            from: self.id,
                            to: peer_id.clone(),
                            term: self.term,
                            message: Message::AppendEntriesRequest {
                                prev_index: prev_index,
                                prev_term: self.log.term_at(prev_index),
                                entries: self.log.entries(prev_index, last_index),
                                commit_index: std::cmp::min(self.commit_index, last_index)
                            }
                        });
                        peer.rpc_alarm = Alarm::new(model.time() + model.rpc_timeout());
                        peer.heartbeat_alarm = Alarm::new(model.time() + model.max_election_timeout() / 2)
                    },
                    _ => {}
                },
            _ => {}
        }
    }

    fn advance_commit_index(&mut self) {
        match self.state {
            ServerState::Leader => {
                let quorum_size = self.peers.len().clone() / 2;
                let median_match_index = self.peers.iter()
                    .map(|(_,peer)| peer.match_index)
                    .chain(std::iter::once(self.log.length()))
                    .sorted()
                    .into_iter()
                    .nth(quorum_size);
                match median_match_index {
                    Option::Some(match_index) if self.log.term_at(match_index) == self.term => {
                        self.commit_index = std::cmp::max(self.commit_index, match_index);
                    },
                    _ => {}
                }
            },
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
    }
}
