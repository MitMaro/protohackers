use std::{
	io::{Read, Write},
	net::{Shutdown, TcpStream},
};

use anyhow::Error;

pub(crate) fn handle(mut stream: TcpStream, _id: usize) -> Result<(), Error> {
	let mut buffer = [0; 128];

	while let Ok(size) = stream.read(&mut buffer) {
		let _ = stream.write_all(&buffer[0..size])?;
		let _ = stream.flush()?;
		if size == 0 {
			break;
		}
	}
	let _ = stream.shutdown(Shutdown::Read)?;
	Ok(())
}
