use std::{num::NonZeroUsize, sync::Arc};

use crossbeam::channel::{unbounded, Sender};
use parking_lot::Mutex;

use crate::{job::Job, worker::Worker};

pub(crate) struct ThreadPool {
	workers: Vec<Worker>,
	sender: Option<Sender<Job>>,
}

impl ThreadPool {
	pub(crate) fn new(size: NonZeroUsize) -> ThreadPool {
		let (sender, receiver) = unbounded();

		let receiver = Arc::new(Mutex::new(receiver));
		let mut workers = Vec::with_capacity(size.get());

		for id in 0..size.get() {
			workers.push(Worker::new(id, Arc::clone(&receiver)));
		}

		ThreadPool {
			workers,
			sender: Some(sender),
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

		for worker in &mut self.workers {
			if let Some(thread) = worker.take() {
				thread.join().unwrap();
			}
		}
	}
}
