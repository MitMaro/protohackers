use std::{
	io::{ErrorKind, Read, Write},
	net::{Shutdown, TcpStream},
	time::Duration,
};

use anyhow::{Error, Result};

pub(crate) fn handle(mut stream: TcpStream, id: u32) -> Result<()> {
	stream.set_read_timeout(Some(Duration::from_millis(60000)))?;
	let mut data_read = false;
	let mut values = vec![];

	'main: loop {
		let mut buffer = [0; 9];
		eprintln!("({id}) Reading data");
		match stream.read_exact(&mut buffer) {
			Ok(_) => {
				data_read = true;
			},
			Err(ref err) if err.kind() == ErrorKind::WouldBlock => {
				if data_read {
					break 'main;
				}
				continue;
			},
			Err(err) => {
				eprintln!("{}", err);
				return Err(Error::from(err));
			},
		}
		eprintln!("({id}) Buffer: '{:x?}' ", buffer);

		let op_type = buffer[0];
		let first = i32::from_be_bytes([buffer[1], buffer[2], buffer[3], buffer[4]]);
		let second = i32::from_be_bytes([buffer[5], buffer[6], buffer[7], buffer[8]]);
		if op_type == b'I' {
			values.push((first, second));
			eprintln!("({id}) OP: I, Timestamp: {first}, Amount: {second}");
		}
		else if op_type == b'Q' {
			let mut average: f64 = 0.0;
			let mut count = 0;
			for (time, value) in &values {
				if (first..=second).contains(&time) {
					average = (count as f64 * average + (*value as f64)) / (count as f64 + 1.0);
					count += 1;
				}
			}
			let mean = average.round() as i32;
			eprintln!("({id}) OP: Q, Start: {first}, End: {second}, Mean: {mean}");
			stream.write_all(&mean.to_be_bytes())?;
		}
		else {
			eprintln!("({id}) Ignoring Op: {op_type}");
			break;
		}
	}
	eprintln!("({id}) Shutting down");
	stream.flush()?;
	stream.shutdown(Shutdown::Read)?;
	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn tests() {}
}