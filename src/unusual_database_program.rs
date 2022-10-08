use std::{
	collections::HashMap,
	net::{SocketAddr, UdpSocket},
};

use anyhow::Error;
use parking_lot::Mutex;

use crate::UdpHandler;

const VERSION: &str = "MM Key-Value Store: 1.0.0";

pub(crate) struct UnusualDatabaseProgram {
	data: Mutex<HashMap<String, String>>,
}

impl UnusualDatabaseProgram {
	pub(crate) fn new() -> Self {
		Self {
			data: Mutex::new(HashMap::new()),
		}
	}
}

impl UdpHandler for UnusualDatabaseProgram {
	fn handler(&self, data: &[u8], socket: &mut UdpSocket, addr: SocketAddr) -> Result<(), Error> {
		let message = String::from(String::from_utf8_lossy(data));

		if message == "version" {
			eprintln!("Write: {}", VERSION);
			let _ = socket.send_to(format!("version={}", VERSION).as_bytes(), addr)?;
			return Ok(());
		}

		if message.contains('=') {
			let mut message_parsed = message.splitn(2, '=');
			let key = message_parsed.next().unwrap_or_default();
			let value = message_parsed.next().unwrap_or_default();
			eprintln!("Write: {key} = '{value}'");
			let _prev = self.data.lock().insert(String::from(key), String::from(value));
		}
		else {
			let data_hashmap = self.data.lock();
			eprintln!("Get: {message}");
			if let Some(value) = data_hashmap.get(&message) {
				let _ = socket.send_to(format!("{message}={value}").as_bytes(), addr)?;
			}
			else {
				let _ = socket.send_to(format!("{message}=").as_bytes(), addr)?;
			}
		}
		Ok(())
	}
}
