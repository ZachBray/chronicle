use time::Duration;

use super::measures::{LogIndex, NodeId};

pub struct Configuration<'a> {
    pub node_id: NodeId,
    pub max_election_timeout: Duration,
    pub rpc_timeout: Duration,
    pub batch_size: LogIndex,
    pub peer_ids: &'a [NodeId],
}
