pub(crate) fn data_to_hex(data: &[u8]) -> String {
	let mut hex = String::new();

	for v in data {
		hex.push_str(format!("{:02X} ", v).as_str());
	}

	String::from(hex.trim())
}
