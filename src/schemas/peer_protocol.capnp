@0xd0bb87029c6e658a;

struct Envelope {
  term @0 :UInt64;
  message :union {
    requestVoteRequest @1 :RequestVoteRequest;
    requestVoteReply @2: RequestVoteReply;
    appendEntriesRequest @3: AppendEntriesRequest;
    appendEntriesReply @4: AppendEntriesReply;
  }
}

struct RequestVoteRequest {
  lastLogTerm @0 :UInt64;
  lastLogIndex @1 :UInt64;
}

struct RequestVoteReply {
  granted @0 :Bool;
}

struct AppendEntriesRequest {
  prevIndex @0 :UInt64;
  prevTerm @1 :UInt64;
  commitIndex @2 :UInt64;
  entries @3 :List(LogEntry);

  struct LogEntry {
    term @0 :UInt64;
    data @1 :Data;
  }
}

struct AppendEntriesReply {
  success @0 :Bool;
  matchIndex @1 :UInt64;
}
