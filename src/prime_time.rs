use std::{
	io::{Read, Write},
	iter::Peekable,
	net::{Shutdown, TcpStream},
	str::Chars,
	time::Duration,
};

use anyhow::{anyhow, Result};
use num::{BigUint, Integer, Zero};

#[derive(Debug, Eq, PartialEq)]
struct Request {
	method: String,
	number: String,
}

#[derive(Debug, Eq, PartialEq)]
enum ParseState {
	Key,
	Value,
}

fn is_prime(x: u128) -> bool {
	if x <= 1 {
		return false;
	}
	if x % 2 == 0 {
		return x == 2;
	};
	// check every odd number, up to sqrt(x)
	for n in (3..).step_by(2).take_while(|m| m * m <= x) {
		if x % n == 0 {
			return n == x;
		};
	}
	true
}

fn is_prime_big_int(x: BigUint) -> bool {
	// some assumptions can be made here, since small numbers will not be passed to this function
	if x.mod_floor(&BigUint::from(2u8)).is_zero() {
		return false;
	};

	let limit = x.sqrt();

	let mut next = BigUint::from(3u8);
	while next <= limit {
		if x.mod_floor(&next).is_zero() {
			return next == x;
		};

		next += 2u8;
	}
	true
}

fn skip_whitespace(chars: &mut Peekable<Chars<'_>>) {
	while let Some(c) = chars.peek() {
		if !c.is_whitespace() {
			return;
		}
		let _ = chars.next();
	}
}

fn assert_next(chars: &mut Peekable<Chars<'_>>, value: char) -> Result<()> {
	skip_whitespace(chars);
	if let Some(c) = chars.next() {
		if c == value {
			Ok(())
		}
		else {
			Err(anyhow!("Malformed JSON: '{}' expected, '{}' found", value, c))
		}
	}
	else {
		Err(anyhow!("Malformed JSON: unexpected end of token stream"))
	}
}

fn is_next(chars: &mut Peekable<Chars<'_>>, value: char) -> bool {
	skip_whitespace(chars);
	chars.peek().map_or(false, |c| *c == value)
}

fn read_value(chars: &mut Peekable<Chars<'_>>, is_string: bool) -> Result<String> {
	let mut escaped = false;
	let mut value = String::new();

	// skip first "
	if is_string {
		assert_next(chars, '"')?;
	}

	while let Some(c) = chars.peek().copied() {
		if escaped {
			value.push(c);
			escaped = false;
		}
		// end value tokens
		else if is_string && c == '"' {
			let _ = chars.next(); // skip "
			return Ok(value);
		}
		else if !is_string && (c == '}' || c == ',') {
			return Ok(value);
		}
		else if c == '\\' {
			escaped = true
		}
		else {
			value.push(c)
		}
		let _ = chars.next();
	}
	Err(anyhow!("Malformed JSON: unexpected end of token stream"))
}

fn skip_object_value(chars: &mut Peekable<Chars<'_>>) -> Result<()> {
	let mut brackets = 0;

	while let Some(c) = chars.peek().copied() {
		if c == '"' {
			let _ = read_value(chars, true)?;
			continue;
		}
		else if c == '}' {
			brackets -= 1;
		}
		else if c == '{' {
			brackets += 1;
		}

		if brackets == 0 {
			return Ok(());
		}

		let _ = chars.next();
	}
	Err(anyhow!("Malformed JSON: unexpected end of token stream"))
}

fn parse_json(line: &str) -> Result<Request> {
	let mut key = String::new();
	let mut state = ParseState::Key;
	let mut method = String::new();
	let mut number = String::new();

	let mut tokens = line.chars().peekable();

	assert_next(&mut tokens, '{')?;

	while tokens.peek().is_some() {
		if is_next(&mut tokens, '}') {
			break;
		}

		match state {
			ParseState::Key => {
				skip_whitespace(&mut tokens);
				key = read_value(&mut tokens, true)?;
				state = ParseState::Value;
			},
			ParseState::Value => {
				state = ParseState::Key;
				assert_next(&mut tokens, ':')?;
				skip_whitespace(&mut tokens);
				if is_next(&mut tokens, '{') {
					skip_object_value(&mut tokens)?;
					assert_next(&mut tokens, '}')?;
					// if not end or JSON, we expect a ,
					skip_whitespace(&mut tokens);
					if !is_next(&mut tokens, '}') {
						assert_next(&mut tokens, ',')?;
					}

					continue;
				}

				let is_string = is_next(&mut tokens, '"');
				let value = read_value(&mut tokens, is_string)?;

				// this assumes no duplicate keys
				match key.as_str() {
					"method" => method = value,
					"number" => number = if is_string { format!("\"{}\"", value) } else { value },
					_ => {},
				}

				// if not the end of the object, expect a ,
				skip_whitespace(&mut tokens);
				if !is_next(&mut tokens, '}') {
					assert_next(&mut tokens, ',')?;
				}
			},
		}
	}
	assert_next(&mut tokens, '}')?;

	Ok(Request { method, number })
}

fn handle_request_data(request: Result<Request>) -> Result<String> {
	let r = request?;
	if r.method != "isPrime" {
		return Err(anyhow!("Malformed request: invalid method {}", r.method));
	}

	if r.number.contains('"') || r.number.parse::<f64>().is_err() {
		return Err(anyhow!("Malformed request: invalid number {}", r.number));
	}

	let prime = if r.number.contains('.') || r.number.contains('-') {
		"false"
	}
	else {
		if let Ok(number) = r.number.parse::<u128>() {
			if is_prime(number) { "true" } else { "false" }
		}
		else {
			let number = BigUint::parse_bytes(r.number.as_bytes(), 10).unwrap();
			if is_prime_big_int(number) { "true" } else { "false" }
		}
	};

	Ok(format!("{{\"method\": \"isPrime\", \"prime\": {}}}\n", prime))
}

pub(crate) fn handle(mut stream: TcpStream, id: usize) -> Result<()> {
	let mut buffer = [0; 4068];
	stream.set_read_timeout(Some(Duration::new(5, 0)))?;

	'main: loop {
		eprintln!("({id}) Reading data");
		let mut data = String::new();
		while let Ok(size) = stream.read(&mut buffer) {
			data.push_str(String::from_utf8_lossy(&buffer[0..size]).as_ref());

			if size == 0 || data.ends_with('\n') {
				break;
			}
		}

		eprintln!("({id}) Data: '{}' ", data.trim());

		if data.trim().is_empty() {
			stream.write_all("MALFORMED: Empty".as_bytes())?;
			break;
		}

		for line in data.lines() {
			match handle_request_data(parse_json(line)) {
				Ok(out) => {
					eprintln!("({id}) Data: {data} Result: {out}");
					stream.write_all(out.as_bytes())?;
				},
				Err(err) => {
					eprintln!("({id}) Data: {data} Error: {}", err.to_string());
					stream.write_all(err.to_string().as_bytes())?;
					break 'main;
				},
			}

			stream.flush()?;
		}
	}
	eprintln!("({id}) Shutting down");
	stream.shutdown(Shutdown::Read)?;
	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn tests() {
		assert_eq!(parse_json("{}").unwrap(), Request {
			method: String::new(),
			number: String::new()
		});
		assert_eq!(parse_json("{\"key\": \"value\"}").unwrap(), Request {
			method: String::new(),
			number: String::new()
		});
		assert_eq!(parse_json("{\"k\": 1}").unwrap(), Request {
			method: String::new(),
			number: String::new()
		});
		assert_eq!(parse_json("{\"k\": 1, \"k2\": 2}").unwrap(), Request {
			method: String::new(),
			number: String::new()
		});
		assert_eq!(
			parse_json("{\"method\": \"isPrime\", \"number\": 123}").unwrap(),
			Request {
				method: String::from("isPrime"),
				number: String::from("123")
			}
		);
		assert_eq!(
			parse_json("{\"k\": {}, \"method\": \"isPrime\", \"number\": 123}").unwrap(),
			Request {
				method: String::from("isPrime"),
				number: String::from("123")
			}
		);
		assert_eq!(
			parse_json("{\"k\": {}, \"method\": \"isPrime\", \"number\": 123, \"k\": {}}").unwrap(),
			Request {
				method: String::from("isPrime"),
				number: String::from("123")
			}
		);
		assert_eq!(
			parse_json("{\"k\": {\"method\":\"not{Prime\"}, \"method\": \"isPrime\", \"number\": 123}").unwrap(),
			Request {
				method: String::from("isPrime"),
				number: String::from("123")
			}
		);
	}

	#[test]
	fn handle_request_big_number() {
		assert_eq!(
			handle_request_data(Ok(Request {
				method: String::from("isPrime"),
				number: String::from("465664798725654230307600049329275256128334622729792136683073")
			}))
			.unwrap(),
			"{\"method\": \"isPrime\", \"prime\": false}\n"
		);
	}

	#[test]
	fn prime_test() {
		assert!(is_prime(2));
	}
}
