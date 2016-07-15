use std;
use std::collections::HashMap;

use time::Duration;
use itertools::Itertools;

use super::measures::{NodeId, Term, LogIndex};
use super::log::Log;
use super::alarm::Alarm;
use super::configuration::Configuration;
use super::network::Network;
use super::messages::{Envelope, Header, Message, RequestVoteRequest, RequestVoteReply,
                      AppendEntriesRequest, AppendEntriesReply};

enum NodeState {
    Follower,
    Candidate,
    Leader,
}

struct Peer {
    vote_granted: bool,
    match_index: LogIndex,
    next_index: LogIndex,
    rpc_alarm: Alarm,
    heartbeat_alarm: Alarm,
}

impl Peer {
    fn new() -> Self {
        Peer {
            vote_granted: false,
            match_index: 0,
            next_index: 1,
            rpc_alarm: Alarm::due_now(),
            heartbeat_alarm: Alarm::due_now(),
        }
    }

    fn reset(&mut self) {
        self.vote_granted = false;
        self.match_index = 0;
        self.next_index = 1;
        self.rpc_alarm = Alarm::due_now();
        self.heartbeat_alarm = Alarm::due_now();
    }
}

pub struct Node<'a, L, N>
    where L: Log,
          N: Network
{
    term: Term,
    commit_index: LogIndex,
    state: NodeState,
    voted_for: Option<NodeId>,
    election_alarm: Alarm,
    peers: HashMap<NodeId, Peer>,
    time: Duration,
    log: L,
    network: N,
    config: Configuration<'a>,
}

impl<'a, L, N> Node<'a, L, N>
    where L: Log,
          N: Network
{
    pub fn new(log: L, network: N, config: Configuration<'a>) -> Self {
        Node {
            peers: config.peer_ids.iter().fold(HashMap::new(), |mut acc, &p| {
                acc.insert(p, Peer::new());
                acc
            }),
            state: NodeState::Follower,
            term: 1,
            voted_for: Option::None,
            commit_index: 0,
            election_alarm: Alarm::due_now(),
            time: Duration::min_value(),
            log: log,
            network: network,
            config: config,
        }
    }

    fn set_random_election_alarm(&mut self) {
        self.election_alarm = Alarm::due_between(self.time,
                                                 self.time + self.config.max_election_timeout);
    }

    fn step_down(&mut self, term: u64) {
        self.term = term;
        self.state = NodeState::Follower;
        self.voted_for = Option::None;
        if self.election_alarm.is_due(self.time) {
            self.set_random_election_alarm();
        }
    }

    fn start_new_election(&mut self) {
        match self.state {
            NodeState::Follower | NodeState::Candidate if self.election_alarm
                .is_due(self.time) => {
                self.set_random_election_alarm();
                self.term += 1;
                self.voted_for = Option::Some(self.config.node_id);
                self.state = NodeState::Candidate;
                for (_, peer) in &mut self.peers {
                    peer.reset();
                }
            }
            _ => {}
        }
    }

    fn send_request_vote(&mut self, peer_id: &NodeId) {
        if let NodeState::Candidate = self.state {
            if let Option::Some(peer) = self.peers.get_mut(peer_id) {
                if peer.rpc_alarm.is_due(self.time) {
                    peer.rpc_alarm = Alarm::new(self.time + self.config.rpc_timeout);
                    self.network.send(Envelope {
                        header: Header {
                            from: self.config.node_id,
                            to: *peer_id,
                            term: self.term,
                        },
                        message: Message::RequestVoteRequest(RequestVoteRequest {
                            last_log_term: self.log.term(),
                            last_log_index: self.log.length(),
                        }),
                    });
                }
            }
        }
    }

    fn become_leader(&mut self) {
        if let NodeState::Candidate = self.state {
            let votes_for_this_server = self.peers.iter().filter(|&(_, p)| p.vote_granted).count();
            let num_servers = self.peers.len();
            let has_quorum = votes_for_this_server + 1 > (num_servers / 2);
            if has_quorum {
                self.state = NodeState::Leader;
                for (_, peer) in &mut self.peers {
                    peer.next_index = self.log.length() + 1;
                    peer.rpc_alarm = Alarm::due_never();
                    peer.heartbeat_alarm = Alarm::due_now();
                }
                self.election_alarm = Alarm::due_never();
            }
        }
    }

    fn send_append_entries(&mut self, peer_id: &NodeId) {
        if let Option::Some(peer) = self.peers.get_mut(peer_id) {
            match self.state {
                NodeState::Leader if peer.heartbeat_alarm.is_due(self.time) ||
                                     (peer.rpc_alarm.is_due(self.time) &&
                                      peer.next_index <= self.log.length()) => {
                    let prev_index = peer.next_index - 1;
                    let last_index = if peer.match_index + 1 < peer.next_index {
                        prev_index
                    } else {
                        std::cmp::min(prev_index + self.config.batch_size, self.log.length())
                    };
                    self.network.send(Envelope {
                        header: Header {
                            from: self.config.node_id,
                            to: *peer_id,
                            term: self.term,
                        },
                        message: Message::AppendEntriesRequest(AppendEntriesRequest {
                            prev_index: prev_index,
                            prev_term: self.log.term_at(prev_index),
                            entries: self.log.entries(prev_index, last_index),
                            commit_index: std::cmp::min(self.commit_index, last_index),
                        }),
                    });
                    peer.rpc_alarm = Alarm::new(self.time + self.config.rpc_timeout);
                    peer.heartbeat_alarm = Alarm::new(self.time +
                                                      self.config.max_election_timeout / 2)
                }
                _ => {}
            }
        }
    }

    fn advance_commit_index(&mut self) {
        if let NodeState::Leader = self.state {
            let quorum_size = self.peers.len() / 2;
            let median_match_index = self.peers
                .iter()
                .map(|(_, peer)| peer.match_index)
                .chain(std::iter::once(self.log.length()))
                .sorted()
                .into_iter()
                .nth(quorum_size);
            match median_match_index {
                Option::Some(match_index) if self.log.term_at(match_index) == self.term => {
                    self.commit_index = std::cmp::max(self.commit_index, match_index);
                }
                _ => {}
            }
        }
    }

    fn handle_request_vote_request(&mut self, header: &Header, message: &RequestVoteRequest) {
        if self.term < header.term {
            self.step_down(header.term);
        }
        let has_same_term = self.term == header.term;
        let this_server_did_not_vote = self.voted_for.is_none();
        let this_server_did_vote_for_them = match self.voted_for { 
            Option::Some(peer_id) if peer_id == header.from => true,
            _ => false, 
        };
        let log_term = self.log.term();
        let that_server_has_at_least_as_many_entries =
            message.last_log_term > log_term ||
            (message.last_log_term == log_term && message.last_log_index >= self.log.length());
        let granted = has_same_term &&
                      (this_server_did_not_vote || this_server_did_vote_for_them) &&
                      that_server_has_at_least_as_many_entries;
        if granted {
            self.voted_for = Option::Some(header.from);
            self.set_random_election_alarm();
        }
        self.network.send(Envelope {
            header: Header {
                to: header.from,
                from: self.config.node_id,
                term: self.term,
            },
            message: Message::RequestVoteReply(RequestVoteReply { granted: granted }),
        });
    }

    fn handle_request_vote_reply(&mut self, header: &Header, message: &RequestVoteReply) {
        match self.state {
            NodeState::Candidate if self.term == header.term => {
                if let Option::Some(peer) = self.peers.get_mut(&header.from) {
                    peer.rpc_alarm = Alarm::due_never();
                    peer.vote_granted = message.granted;
                }
            }
            _ if self.term < header.term => {
                self.step_down(header.term);
            }
            _ => {}
        }
    }

    fn handle_append_entries_request(&mut self, header: &Header, message: &AppendEntriesRequest) {
        let mut success = false;
        let mut match_index = 0;
        match header.term {
            t if t > self.term => self.step_down(t),
            t if t == self.term => {
                self.state = NodeState::Follower;
                self.set_random_election_alarm();
                if message.prev_index == 0 ||
                   (message.prev_index <= self.log.length() &&
                    self.log.term_at(message.prev_index) == message.prev_term) {
                    success = true;
                    match_index = message.prev_index;
                    for entry in message.entries {
                        match_index += 1;
                        if self.log.term_at(match_index) != entry.term {
                            self.log.truncate(match_index);
                            self.log.append(entry);
                        }
                    }
                    self.commit_index = std::cmp::max(self.commit_index, message.commit_index);
                }
            }
            _ => {}
        }
        self.network.send(Envelope {
            header: Header {
                from: self.config.node_id,
                to: header.from,
                term: self.term,
            },
            message: Message::AppendEntriesReply(AppendEntriesReply {
                success: success,
                match_index: match_index,
            }),
        });
    }

    fn handle_append_entries_reply(&mut self, header: &Header, message: &AppendEntriesReply) {
        match self.state {
            NodeState::Leader if self.term == header.term => {
                if let Option::Some(peer) = self.peers.get_mut(&header.from) {
                    if message.success {
                        peer.match_index = std::cmp::max(peer.match_index, message.match_index);
                        peer.next_index = peer.match_index + 1;
                    } else {
                        peer.next_index = std::cmp::max(1, peer.next_index - 1);
                    }
                    peer.rpc_alarm = Alarm::due_now();
                }
            }
            _ if self.term < header.term => {
                self.step_down(header.term);
            }
            _ => {}
        }
    }

    fn handle_message(&mut self, envelope: &Envelope) {
        match envelope.message {
            Message::RequestVoteRequest(ref message) => {
                self.handle_request_vote_request(&envelope.header, message)
            }
            Message::RequestVoteReply(ref message) => {
                self.handle_request_vote_reply(&envelope.header, message)
            }
            Message::AppendEntriesRequest(ref message) => {
                self.handle_append_entries_request(&envelope.header, message)
            }
            Message::AppendEntriesReply(ref message) => {
                self.handle_append_entries_reply(&envelope.header, message)
            }
        }
    }

    pub fn run(&mut self, time: Duration, message: Option<&Envelope>) {
        self.time = time;
        self.start_new_election();
        self.become_leader();
        self.advance_commit_index();
        for m in message {
            self.handle_message(m);
        }
        // TODO: Avoid creating a vector here if possible. E.g., move the peer list outside of the
        // main state.
        let peers: Vec<NodeId> = self.peers.iter().map(|(peer_id, _)| peer_id.clone()).collect();
        for peer_id in peers {
            self.send_request_vote(&peer_id);
            self.send_append_entries(&peer_id);
        }
    }
}
