use std;

use time::Duration;
use itertools::Itertools;

use super::measures::{NodeId, Term, LogIndex};
use super::alarm::Alarm;
use super::configuration::Configuration;
use super::log::Log;
use super::network::Network;
use super::peer::Peer;
use super::messages::{Envelope, Header, Message, RequestVoteRequest, RequestVoteReply,
                      AppendEntriesRequest, AppendEntriesReply};

enum NodeState {
    Follower,
    Candidate,
    Leader,
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

    fn start_new_election(&mut self, peers: &mut Vec<Peer>) {
        if self.election_alarm.is_due(self.time) {
            self.set_random_election_alarm();
            self.term += 1;
            self.voted_for = Option::Some(self.config.node_id);
            self.state = NodeState::Candidate;
            for mut peer in peers {
                peer.reset();
            }
        }
    }

    fn send_request_vote(&mut self, peer: &mut Peer) {
        if peer.rpc_alarm.is_due(self.time) {
            peer.rpc_alarm = Alarm::new(self.time + self.config.rpc_timeout);
            self.network.send(Envelope {
                header: Header {
                    from: self.config.node_id,
                    to: peer.id,
                    term: self.term,
                },
                message: Message::RequestVoteRequest(RequestVoteRequest {
                    last_log_term: self.log.term(),
                    last_log_index: self.log.length(),
                }),
            });
        }
    }

    fn become_leader(&mut self, peers: &mut Vec<Peer>) {
        let votes_for_this_server = peers.iter().filter(|&p| p.vote_granted).count();
        let num_servers = peers.len();
        let has_quorum = votes_for_this_server + 1 > (num_servers / 2);
        if has_quorum {
            self.state = NodeState::Leader;
            for mut peer in peers {
                peer.next_index = self.log.length() + 1;
                peer.rpc_alarm = Alarm::due_never();
                peer.heartbeat_alarm = Alarm::due_now();
            }
            self.election_alarm = Alarm::due_never();
        }
    }

    fn send_append_entries(&mut self, peer: &mut Peer) {
        if peer.heartbeat_alarm.is_due(self.time) ||
           (peer.rpc_alarm.is_due(self.time) && peer.next_index <= self.log.length()) {
            let prev_index = peer.next_index - 1;
            let last_index = if peer.match_index + 1 < peer.next_index {
                prev_index
            } else {
                std::cmp::min(prev_index + self.config.batch_size, self.log.length())
            };
            self.network.send(Envelope {
                header: Header {
                    from: self.config.node_id,
                    to: peer.id,
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
            peer.heartbeat_alarm = Alarm::new(self.time + self.config.max_election_timeout / 2)
        }
    }

    fn advance_commit_index(&mut self, peers: &mut Vec<Peer>) {
        let quorum_size = peers.len() / 2;
        let median_match_index = peers.iter()
            .map(|peer| peer.match_index)
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

    fn handle_request_vote_reply(&mut self,
                                 peers: &mut Vec<Peer>,
                                 header: &Header,
                                 message: &RequestVoteReply) {
        match self.state {
            NodeState::Candidate if self.term == header.term => {
                for mut peer in peers {
                    if peer.id == header.from {
                        peer.rpc_alarm = Alarm::due_never();
                        peer.vote_granted = message.granted;
                    }
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

    fn handle_append_entries_reply(&mut self,
                                   peers: &mut Vec<Peer>,
                                   header: &Header,
                                   message: &AppendEntriesReply) {
        match self.state {
            NodeState::Leader if self.term == header.term => {
                for mut peer in peers {
                    if peer.id == header.from {
                        if message.success {
                            peer.match_index = std::cmp::max(peer.match_index, message.match_index);
                            peer.next_index = peer.match_index + 1;
                        } else {
                            peer.next_index = std::cmp::max(1, peer.next_index - 1);
                        }
                        peer.rpc_alarm = Alarm::due_now();
                    }
                }
            }
            _ if self.term < header.term => {
                self.step_down(header.term);
            }
            _ => {}
        }
    }

    pub fn handle_message(&mut self, peers: &mut Vec<Peer>, envelope: &Envelope) {
        match envelope.message {
            Message::RequestVoteRequest(ref message) => {
                self.handle_request_vote_request(&envelope.header, message)
            }
            Message::RequestVoteReply(ref message) => {
                self.handle_request_vote_reply(peers, &envelope.header, message)
            }
            Message::AppendEntriesRequest(ref message) => {
                self.handle_append_entries_request(&envelope.header, message)
            }
            Message::AppendEntriesReply(ref message) => {
                self.handle_append_entries_reply(peers, &envelope.header, message)
            }
        }
    }

    pub fn run(&mut self, peers: &mut Vec<Peer>, time: Duration) {
        self.time = time;
        match self.state {
            NodeState::Follower => {
                self.start_new_election(peers);
            }
            NodeState::Candidate => {
                self.start_new_election(peers);
                self.become_leader(peers);
                for ref mut peer in peers {
                    self.send_request_vote(peer);
                }
            }
            NodeState::Leader => {
                self.advance_commit_index(peers);
                for ref mut peer in peers {
                    self.send_append_entries(peer);
                }
            }
        }
    }
}
