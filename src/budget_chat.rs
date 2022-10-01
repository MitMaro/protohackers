use std::{
	collections::HashMap,
	io::{Read, Write},
	net::{Shutdown, TcpStream},
	sync::Arc,
	thread::{scope, Scope, ScopedJoinHandle},
};

use anyhow::Error;
use crossbeam::channel::{unbounded, Receiver, Sender};
use parking_lot::Mutex;

use crate::handler::Handler;

#[derive(Debug, PartialEq, Eq, Clone)]
pub(crate) enum Message {
	Join(usize),
	Leave(usize, String),
	Message(usize, String),
	Shutdown,
}

#[derive(Debug)]
struct User {
	name: String,
	sender: Sender<Message>,
	receiver: Receiver<Message>,
}

impl User {
	fn new(name: &str) -> Self {
		let (sender, receiver) = unbounded();
		Self {
			name: String::from(name),
			sender,
			receiver,
		}
	}

	fn is_valid_name(name: &str) -> bool {
		if name.is_empty() {
			return false;
		}
		for char in name.chars() {
			if !char.is_alphanumeric() {
				return false;
			}
		}
		true
	}
}

#[derive(Debug, Clone)]
pub(crate) struct BudgetChat {
	next_id: Arc<Mutex<usize>>,
	users: Arc<Mutex<HashMap<usize, User>>>,
}

impl BudgetChat {
	pub(crate) fn new() -> Self {
		Self {
			next_id: Arc::new(Mutex::new(1)),
			users: Arc::new(Mutex::new(HashMap::new())),
		}
	}

	fn next_id(&self) -> usize {
		let mut next_id = self.next_id.lock();
		let id = *next_id;
		*next_id += 1;
		id
	}

	fn add_user(&self, name: &str) -> usize {
		let user_id = self.next_id();
		let user = User::new(name);
		let mut users = self.users.lock();
		let _prev = (*users).insert(user_id, user);
		drop(users);
		self.broadcast(&Message::Join(user_id));
		user_id
	}

	fn remove_user(&self, user_id: usize) {
		let name = self.name(user_id);
		self.broadcast(&Message::Leave(user_id, name));
		let mut users = self.users.lock();

		let _prev = (*users).remove(&user_id);
	}

	fn name(&self, user_id: usize) -> String {
		let users = self.users.lock();

		(*users)[&user_id].name.clone()
	}

	fn room_list(&self) -> String {
		let users = self.users.lock();

		(*users)
			.values()
			.map(|user| user.name.as_str())
			.collect::<Vec<&str>>()
			.join(", ")
	}

	fn broadcast(&self, message: &Message) {
		let users = self.users.lock();

		for user in (*users).values() {
			user.sender.send(message.clone()).unwrap();
		}
	}

	fn send_message(&self, user_id: usize, message: Message) {
		let users = self.users.lock();

		if let Some(user) = (*users).get(&user_id) {
			user.sender.send(message).unwrap();
		}
	}

	fn start_message_thread<'scope>(
		self,
		scope: &'scope Scope<'scope, '_>,
		id: u32,
		mut stream: TcpStream,
		user_id: usize,
	) -> ScopedJoinHandle<'scope, ()> {
		let users = self.users.lock();
		let receiver = (*users)[&user_id].receiver.clone();
		drop(users);
		scope.spawn(move || {
			'main: loop {
				while let Ok(message) = receiver.recv() {
					match message {
						Message::Join(joined_user_id) => {
							if joined_user_id != user_id {
								let joined_name = self.name(joined_user_id);
								let name = self.name(user_id);
								eprintln!("({id}) ({joined_name}) Entered: {name}");
								stream
									.write_all(format!("* {} has entered the room\n", joined_name).as_bytes())
									.unwrap();
							}
						},
						Message::Leave(left_user_id, name) => {
							eprintln!("{left_user_id} {user_id}");
							if left_user_id == user_id {
								break 'main;
							}
							eprintln!("({id}) ({user_id}) Left: {name}");
							stream
								.write_all(format!("* {} has left the room\n", name).as_bytes())
								.unwrap();
						},
						Message::Message(from_user_id, msg) => {
							if from_user_id != user_id {
								let from_name = self.name(from_user_id);
								let name = self.name(user_id);
								eprintln!("({id}) ({from_name}) --> ({name}) Sending: {msg}");
								stream.write_all(format!("[{from_name}] {msg}\n").as_bytes()).unwrap();
							}
						},
						Message::Shutdown => break 'main,
					}
				}
			}
		})
	}
}

impl Handler for BudgetChat {
	fn handler(&self, mut stream: TcpStream, id: u32) -> Result<(), Error> {
		stream.write_all("Welcome to budgetchat! What shall I call you?\n".as_bytes())?;

		let mut recv_steam = stream.try_clone()?;
		scope(move |s| {
			let mut message_thread_handle = None;
			let mut user_id = 0;
			let mut read_buffer = [0; 128];
			let mut buffer = String::new();
			'main: while let Ok(size) = recv_steam.read(&mut read_buffer) {
				if size == 0 {
					break;
				}
				buffer.push_str(String::from_utf8_lossy(&read_buffer[0..size]).as_ref());

				eprintln!("({id}) Buffer: {}", buffer.replace('\n', "\\n"));
				let last_message_complete = buffer.ends_with('\n');
				let mut messages = buffer.lines().map(String::from).collect::<Vec<String>>();

				if !last_message_complete && !messages.is_empty() {
					buffer = messages.remove(messages.len() - 1);
				}
				else {
					buffer.clear();
				}

				for message in messages {
					eprintln!("({id}) Message: {message}");
					if user_id == 0 {
						let name = message.trim();
						if !User::is_valid_name(name) {
							self.send_message(user_id, Message::Shutdown);
							recv_steam
								.write_all("Name must be provided and must be alphanumeric\n".as_bytes())
								.unwrap();
							recv_steam.shutdown(Shutdown::Read).unwrap();
							break 'main;
						}
						let room_list = self.room_list();
						user_id = self.add_user(name);
						eprintln!("({id}) Joined: {name}, ID: {user_id}, Room: {room_list}");
						recv_steam
							.write_all(format!("* The room contains: {room_list}\n").as_bytes())
							.unwrap();
						message_thread_handle =
							Some(
								self.clone()
									.start_message_thread(s, id, recv_steam.try_clone().unwrap(), user_id),
							);
						continue;
					}
					if !message.starts_with('*') {
						eprintln!("({id}) ({user_id}) Sending: {message}");
						self.broadcast(&Message::Message(user_id, message));
					}
				}
			}

			if user_id != 0 {
				let name = self.name(user_id);
				eprintln!("({id}) Disconnected: {name} ({user_id})");
				self.remove_user(user_id);
			}

			if let Some(handle) = message_thread_handle {
				handle.join().unwrap();
			}
		});

		eprintln!("({id}) Shutdown");
		stream.shutdown(Shutdown::Read)?;

		Ok(())
	}

	fn shutdown(&self) {
		let users = self.users.lock();

		for user in (*users).values() {
			user.sender.send(Message::Shutdown).unwrap();
		}
	}
}
