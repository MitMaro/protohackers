use std::thread::{spawn, JoinHandle};

use captur::capture;
use crossbeam::channel::Receiver;

use crate::job::Job;

pub(crate) struct Worker {
	id: usize,
	thread: Option<JoinHandle<()>>,
}

impl Worker {
	pub(crate) fn new(id: usize, receiver: Receiver<Job>) -> Worker {
		let thread = spawn(move || {
			loop {
				capture!(receiver);
				eprintln!("Worker waiting: {}", id);

				match receiver.recv() {
					Ok(job) => {
						eprintln!("Starting job on worker: {}", id);
						job();
						eprintln!("Ending job on worker: {}", id);
					},
					Err(_) => break,
				}
			}
		});

		Worker {
			id,
			thread: Some(thread),
		}
	}

	pub(crate) fn id(&self) -> usize {
		self.id
	}

	pub(crate) fn take(&mut self) -> Option<JoinHandle<()>> {
		self.thread.take()
	}
}
