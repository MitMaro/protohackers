use std::thread::{spawn, JoinHandle};

use captur::capture;
use crossbeam::channel::Receiver;

use crate::job::Job;

pub(crate) struct Worker {
	_id: usize,
	thread: Option<JoinHandle<()>>,
}

impl Worker {
	pub(crate) fn new(id: usize, receiver: Receiver<Job>) -> Worker {
		let thread = spawn(move || {
			loop {
				capture!(receiver);
				eprintln!("Worker waiting: {}", id);

				if let Ok(job) = receiver.recv() {
					eprintln!("Starting job on worker: {}", id);
					job();
					eprintln!("Ending job on worker: {}", id);
				}
			}
		});

		Worker {
			_id: id,
			thread: Some(thread),
		}
	}

	pub(crate) fn take(&mut self) -> Option<JoinHandle<()>> {
		self.thread.take()
	}
}
