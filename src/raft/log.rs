use super::measures::{Term, LogIndex};
use super::messages::LogEntry;

pub trait Log {
    fn term(&self) -> Term {
        self.term_at(self.length())
    }

    fn term_at(&self, index: LogIndex) -> Term;
    fn length(&self) -> LogIndex;
    fn entries<'a>(&'a self,
                   from_index_incl: LogIndex,
                   until_index_excl: LogIndex)
                   -> &'a [&'a LogEntry];
    fn truncate(&mut self, to_index_incl: LogIndex);
    fn append(&mut self, entry: &LogEntry);
}
