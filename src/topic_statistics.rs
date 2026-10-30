// Copyright (C) 2020, 2026 Open Source Robotics Foundation
// SPDX-License-Identifier: Apache-2.0
use crate::{Error, SampleStatistics};
use std::collections::HashMap;

/// Owned bounded topic samples, with explicit native-epoch millisecond inputs.
///
/// This value does not read clocks or attach to a Node. Sender identity is exact
/// bytes, including embedded NUL and invalid UTF-8. Cloning copies all history.
#[derive(Clone, Debug)]
pub struct TopicStatistics {
    sequences: HashMap<Vec<u8>, u64>,
    sender_bytes: usize,
    max_senders: usize,
    max_sender_bytes: usize,
    publication: SampleStatistics,
    reception: SampleStatistics,
    age: SampleStatistics,
    dropped: u64,
    previous_publication: u64,
    previous_reception: u64,
}

impl TopicStatistics {
    /// Bound both sender cardinality and the total retained sender-name bytes.
    /// Zero cardinality rejects every update; zero bytes permits empty senders.
    pub fn new(max_senders: usize, max_retained_sender_bytes: usize) -> Self {
        Self {
            sequences: HashMap::new(),
            sender_bytes: 0,
            max_senders,
            max_sender_bytes: max_retained_sender_bytes,
            publication: SampleStatistics::new(),
            reception: SampleStatistics::new(),
            age: SampleStatistics::new(),
            dropped: 0,
            previous_publication: 0,
            previous_reception: 0,
        }
    }

    /// Update using explicit reception time and native publication metadata.
    ///
    /// All times are unsigned milliseconds from the same epoch. Subtraction and
    /// sequence arithmetic wrap as in native. A previous publication stamp of
    /// zero suppresses timing/drop samples, while still retaining new history.
    /// Exhausted sender-count/byte budgets return QueueFull before any mutation.
    /// Existing senders remain admissible when either budget is full.
    pub fn update_at(
        &mut self,
        received_milliseconds: u64,
        sender: impl AsRef<[u8]>,
        publication_milliseconds: u64,
        sequence: u64,
    ) -> Result<(), Error> {
        let sender = sender.as_ref();
        let previous_sequence = self.sequences.get(sender).copied();
        let bytes = if previous_sequence.is_none() {
            let bytes = self
                .sender_bytes
                .checked_add(sender.len())
                .ok_or(Error::QueueFull)?;
            if self.sequences.len() >= self.max_senders || bytes > self.max_sender_bytes {
                return Err(Error::QueueFull);
            }
            bytes
        } else {
            self.sender_bytes
        };
        if self.previous_publication != 0 {
            self.publication
                .update(publication_milliseconds.wrapping_sub(self.previous_publication) as f64);
            self.reception
                .update(received_milliseconds.wrapping_sub(self.previous_reception) as f64);
            self.age
                .update(received_milliseconds.wrapping_sub(publication_milliseconds) as f64);
            if previous_sequence.unwrap_or(0).wrapping_add(1) != sequence {
                self.dropped = self.dropped.wrapping_add(1);
            }
        }
        self.previous_publication = publication_milliseconds;
        self.previous_reception = received_milliseconds;
        if let Some(current) = self.sequences.get_mut(sender) {
            *current = sequence;
        } else {
            self.sequences.insert(sender.to_vec(), sequence);
        }
        self.sender_bytes = bytes;
        Ok(())
    }

    pub fn dropped_msg_count(&self) -> u64 {
        self.dropped
    }

    pub fn publication_statistics(&self) -> SampleStatistics {
        self.publication
    }

    pub fn reception_statistics(&self) -> SampleStatistics {
        self.reception
    }

    pub fn age_statistics(&self) -> SampleStatistics {
        self.age
    }
}

#[cfg(test)]
#[path = "../tests/support/topic_statistics.rs"]
mod tests;
