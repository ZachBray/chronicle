use time::Duration;
use rand::{thread_rng, Rng};

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

    pub fn due_between(start_inclusive: Duration, end_exclusive: Duration) -> Self {
        if start_inclusive >= end_exclusive {
            Alarm::due_now()
        } else {
            let max_millis = end_exclusive.num_milliseconds() - start_inclusive.num_milliseconds();
            let mut rng = thread_rng();
            let due_millis = rng.gen_range(0, max_millis);
            Alarm { due_time: Duration::milliseconds(due_millis) }
        }
    }

    pub fn new(due_time: Duration) -> Self {
        Alarm { due_time: due_time }
    }

    pub fn is_due(&self, time: Duration) -> bool {
        self.due_time <= time
    }
}
