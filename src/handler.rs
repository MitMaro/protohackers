use std::net::TcpStream;

use anyhow::Error;

pub(crate) trait Handler: Send + Sync {
	fn handler(&self, stream: TcpStream, _id: u32) -> Result<(), Error>;

	fn shutdown(&self) {}
}
