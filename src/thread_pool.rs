use std::num::NonZeroUsize;

use crossbeam::channel::{unbounded, Receiver, Sender};

use crate::{job::Job, worker::Worker};

pub(crate) struct ThreadPool {
	workers: Vec<Worker>,
	sender: Option<Sender<Job>>,
	receiver: Option<Receiver<Job>>,
}

impl ThreadPool {
	pub(crate) fn new(size: NonZeroUsize) -> ThreadPool {
		let (sender, receiver) = unbounded();

		let mut workers = Vec::with_capacity(size.get());

		for id in 0..size.get() {
			workers.push(Worker::new(id, receiver.clone()));
		}

		ThreadPool {
			workers,
			sender: Some(sender),
			receiver: Some(receiver),
		}
	}

	pub(crate) fn execute<F>(&self, f: F)
	where F: FnOnce() + Send + 'static {
		self.sender.as_ref().unwrap().send(Box::new(f)).unwrap();
	}
}

impl Drop for ThreadPool {
	fn drop(&mut self) {
		drop(self.sender.take());
		drop(self.receiver.take());

		for worker in &mut self.workers {
			if let Some(thread) = worker.take() {
				thread.join().unwrap();
			}
		}
	}
}
