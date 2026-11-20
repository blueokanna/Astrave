use super::{RESERVE_DAYS, RESERVE_HOURS};
use jiff::{ZonedDateTime, civil::date};

#[derive(Clone, Copy, Default)]
pub(super) struct Bytes {
    pub inbound: u64,
    pub outbound: u64,
}

#[derive(Default)]
pub(super) struct History {
    pub days: Buckets<RESERVE_DAYS>,
    pub hours: Buckets<RESERVE_HOURS>,
}

pub(super) fn day_key(now: &ZonedDateTime) -> i64 {
    i64::from((now.date() - date(1970, 1, 1)).get_days())
}

pub(super) fn hour_start(now: &ZonedDateTime) -> i64 {
    now.timestamp().as_second() - i64::from(now.minute()) * 60 - i64::from(now.second())
}

impl History {
    pub fn add(&mut self, day: i64, hour: i64, bytes: Bytes) {
        self.days.add(day, bytes);
        self.hours.add(hour, bytes);
    }
}

pub(super) struct Buckets<const N: usize> {
    newest: Option<i64>,
    slots: [Bytes; N],
}

impl<const N: usize> Default for Buckets<N> {
    fn default() -> Self {
        Self {
            newest: None,
            slots: [Bytes::default(); N],
        }
    }
}

impl<const N: usize> Buckets<N> {
    fn index(key: i64) -> usize {
        key.rem_euclid(N as i64) as usize
    }

    pub fn get(&self, key: i64) -> Bytes {
        if self
            .newest
            .is_some_and(|last| (0..N as i64).contains(&(last - key)))
        {
            self.slots[Self::index(key)]
        } else {
            Bytes::default()
        }
    }

    fn add(&mut self, key: i64, bytes: Bytes) {
        if let Some(last) = self.newest {
            if last - key >= N as i64 {
                return;
            }
            // Clear only the slots crossed since the last write. Reading a
            // history never rotates or mutates another request's snapshot.
            if key - last >= N as i64 {
                self.slots.fill(Bytes::default());
            } else if key > last {
                for next in last + 1..=key {
                    self.slots[Self::index(next)] = Bytes::default();
                }
            }
        }
        self.newest = Some(self.newest.map_or(key, |last| last.max(key)));
        let slot = &mut self.slots[Self::index(key)];
        slot.inbound = slot.inbound.saturating_add(bytes.inbound);
        slot.outbound = slot.outbound.saturating_add(bytes.outbound);
    }

    pub fn snapshot(&self, end: i64) -> [Bytes; N] {
        std::array::from_fn(|age| self.get(end - age as i64))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calendar_days_and_repeated_local_hours_have_distinct_keys() {
        let early: ZonedDateTime = "2024-11-03T01:30:00-04:00[America/New_York]"
            .parse()
            .unwrap();
        let late: ZonedDateTime = "2024-11-03T01:30:00-05:00[America/New_York]"
            .parse()
            .unwrap();
        let midnight: ZonedDateTime = "2024-11-04T00:00:00-05:00[America/New_York]"
            .parse()
            .unwrap();
        assert_eq!(day_key(&late), day_key(&early));
        assert_eq!(hour_start(&late) - hour_start(&early), 3600);
        assert_eq!(day_key(&midnight) - day_key(&late), 1);
    }

    #[test]
    fn wrap_gap_and_out_of_order_updates_keep_their_own_period() {
        let mut counts = Buckets::<3>::default();
        let bytes = Bytes {
            inbound: 7,
            outbound: 9,
        };
        for hour in [-1, 0, 2, 1, 2] {
            counts.add(hour, bytes);
        }
        assert_eq!(counts.snapshot(2).map(|slot| slot.inbound), [14, 7, 7]);
        assert_eq!(counts.get(-1).outbound, 0);
        assert_eq!(counts.get(3).inbound, 0);
        counts.add(6, bytes);
        counts.add(1, bytes);
        assert_eq!(counts.snapshot(6).map(|slot| slot.outbound), [9, 0, 0]);
    }
}
