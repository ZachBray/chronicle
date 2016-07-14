use time::Duration;

use super::measures::LogIndex;

pub struct Configuration {
    pub max_election_timeout: Duration,
    pub rpc_timeout: Duration,
    pub batch_size: LogIndex,
}
