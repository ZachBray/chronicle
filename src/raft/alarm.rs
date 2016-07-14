use time::Duration;

pub struct Alarm {
    due_time: Duration,
}

impl Alarm {
    pub fn due_now() -> Self {
        Alarm { due_time: Duration::min_value() }
    }

    pub fn due_never() -> Self {
        Alarm { due_time: Duration::max_value() }
    }

    pub fn new(due_time: Duration) -> Self {
        Alarm { due_time: due_time }
    }

    pub fn is_due(&self, time: Duration) -> bool {
        self.due_time <= time
    }
}
