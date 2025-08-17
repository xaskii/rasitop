use std::collections::VecDeque;

use crate::pm::PowermetricsSample;

#[derive(Debug)]
pub struct History {
    capacity: usize,
    samples: VecDeque<PowermetricsSample>,
}

impl History {
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            capacity,
            samples: VecDeque::with_capacity(capacity),
        }
    }

    pub fn push(&mut self, sample: PowermetricsSample) {
        if self.samples.len() == self.capacity {
            self.samples.pop_front();
        }
        self.samples.push_back(sample);
    }

    pub fn len(&self) -> usize {
        self.samples.len()
    }

    pub fn latest(&self) -> Option<&PowermetricsSample> {
        self.samples.back()
    }

    pub fn iter(&self) -> impl Iterator<Item = &PowermetricsSample> {
        self.samples.iter()
    }
}
