use std::{
	sync::Arc,
	thread::{spawn, JoinHandle},
};

use captur::capture;
use crossbeam::channel::Receiver;
use parking_lot::Mutex;

use crate::job::Job;

pub(crate) struct Worker {
	_id: usize,
	thread: Option<JoinHandle<()>>,
}

impl Worker {
	pub(crate) fn new(id: usize, receiver: Arc<Mutex<Receiver<Job>>>) -> Worker {
		let thread = spawn(move || {
			loop {
				capture!(receiver);

				if let Ok(job) = receiver.lock().recv() {
					eprintln!("Starting job on worker: {}", id);
					job();
					eprintln!("Ending job on worker: {}", id);
				}
				break;
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
