use super::measures::{ServerId, Term, LogIndex};

pub struct LogEntry {
    pub term: Term,
    pub data: [u8],
}

pub struct RequestVoteRequest {
    pub last_log_term: Term,
    pub last_log_index: LogIndex,
}

pub struct RequestVoteReply {
    pub granted: bool,
}

pub struct AppendEntriesRequest<'a> {
    pub prev_index: LogIndex,
    pub prev_term: Term,
    pub entries: &'a [&'a LogEntry],
    pub commit_index: LogIndex,
}

pub struct AppendEntriesReply {
    pub success: bool,
    pub match_index: LogIndex,
}

pub enum Message<'a> {
    RequestVoteRequest(RequestVoteRequest),
    RequestVoteReply(RequestVoteReply),
    AppendEntriesRequest(AppendEntriesRequest<'a>),
    AppendEntriesReply(AppendEntriesReply),
}

pub struct Header {
    pub from: ServerId,
    pub to: ServerId,
    pub term: Term,
}

pub struct Envelope<'a> {
    pub header: Header,
    pub message: Message<'a>,
}
