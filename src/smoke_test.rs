use std::{
	io::{Read, Write},
	net::{Shutdown, TcpStream},
};

use anyhow::Error;

use crate::handler::Handler;

#[derive(Debug, Clone)]
pub(crate) struct SmokeTest;

impl SmokeTest {
	pub(crate) fn new() -> Self {
		Self {}
	}
}

impl Handler for SmokeTest {
	fn handler(&self, mut stream: TcpStream, _id: u32) -> Result<(), Error> {
		let mut buffer = [0; 128];

		while let Ok(size) = stream.read(&mut buffer) {
			stream.write_all(&buffer[0..size])?;
			stream.flush()?;
			if size == 0 {
				break;
			}
		}
		stream.shutdown(Shutdown::Read)?;
		Ok(())
	}
}
