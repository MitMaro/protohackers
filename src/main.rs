// enable all rustc's built-in lints
#![deny(
	future_incompatible,
	nonstandard_style,
	rust_2018_compatibility,
	rust_2018_idioms,
	rust_2021_compatibility,
	unused,
	warnings
)]
// rustc's additional allowed by default lints
#![deny(
	absolute_paths_not_starting_with_crate,
	deprecated_in_future,
	elided_lifetimes_in_paths,
	explicit_outlives_requirements,
	keyword_idents,
	macro_use_extern_crate,
	meta_variable_misuse,
	missing_abi,
	missing_copy_implementations,
	missing_debug_implementations,
	non_ascii_idents,
	noop_method_call,
	pointer_structural_match,
	rust_2021_incompatible_closure_captures,
	rust_2021_incompatible_or_patterns,
	rust_2021_prefixes_incompatible_syntax,
	rust_2021_prelude_collisions,
	single_use_lifetimes,
	trivial_casts,
	trivial_numeric_casts,
	unreachable_pub,
	unsafe_code,
	unsafe_op_in_unsafe_fn,
	unstable_features,
	unused_crate_dependencies,
	unused_extern_crates,
	unused_import_braces,
	unused_lifetimes,
	unused_macro_rules,
	unused_qualifications,
	unused_results
)]
// enable all of Clippy's lints
#![deny(clippy::all, clippy::cargo, clippy::pedantic, clippy::restriction)]
#![allow(
	clippy::arithmetic_side_effects,
	clippy::as_conversions,
	clippy::blanket_clippy_restriction_lints,
	clippy::cargo_common_metadata,
	clippy::default_numeric_fallback,
	clippy::else_if_without_else,
	clippy::expect_used,
	clippy::float_arithmetic,
	clippy::implicit_return,
	clippy::indexing_slicing,
	clippy::integer_arithmetic,
	clippy::missing_docs_in_private_items,
	clippy::mod_module_files,
	clippy::module_name_repetitions,
	clippy::option_if_let_else,
	clippy::print_stderr,
	clippy::pub_use,
	clippy::redundant_pub_crate,
	clippy::separated_literal_suffix,
	clippy::std_instead_of_alloc,
	clippy::std_instead_of_core,
	clippy::tabs_in_doc_comments,
	clippy::too_many_lines,
	clippy::unwrap_used
)]

mod budget_chat;
mod handler;
mod job;
mod means_to_an_end;
mod prime_time;
mod smoke_test;
mod thread_pool;
mod unusual_database_program;
mod utils;
mod worker;

use std::{
	collections::HashMap,
	env,
	io::ErrorKind,
	net::{TcpListener, UdpSocket},
	num::NonZeroUsize,
	process,
	sync::{
		atomic::{AtomicBool, Ordering},
		Arc,
	},
	thread,
	time::Duration,
};

use anyhow::{anyhow, Error};
use ctrlc::set_handler;
use lazy_static::lazy_static;
use thread_pool::ThreadPool;

use crate::{
	budget_chat::BudgetChat,
	handler::{TcpHandler, UdpHandler},
	means_to_an_end::MeansToAnEnd,
	prime_time::PrimeTime,
	smoke_test::SmokeTest,
	unusual_database_program::UnusualDatabaseProgram,
	utils::data_to_hex,
};

#[derive(Debug, Copy, Clone)]
enum TcpProblem {
	None,
	SmokeTest,
	PrimeTime,
	MeansToAnEnd,
	BudgetChat,
}

#[derive(Debug, Copy, Clone)]
enum UdpProblem {
	None,
	UnusualDatabaseProgram,
}

#[derive(Debug, Copy, Clone)]
enum Type {
	None,
	Tcp,
	Udp,
}

lazy_static! {
	static ref TCP_PROBLEMS: [(&'static str, TcpProblem); 4] = [
		("smoketest", TcpProblem::SmokeTest),
		("primetime", TcpProblem::PrimeTime),
		("meanstoanend", TcpProblem::MeansToAnEnd),
		("budgetchat", TcpProblem::BudgetChat),
	];
	static ref UDP_PROBLEMS: [(&'static str, UdpProblem); 1] =
		[("unusualdatabaseprogram", UdpProblem::UnusualDatabaseProgram)];
}

#[allow(clippy::exit)]
fn main() {
	if let Err(e) = try_main() {
		eprintln!("{}", e);
		process::exit(1);
	}
}

#[allow(clippy::exit)]
fn try_main() -> Result<(), Error> {
	let port = env::var("PORT").unwrap_or_else(|_| String::from("7878"));
	let shutdown = Arc::new(AtomicBool::new(false));
	let handler_shutdown = Arc::clone(&shutdown);

	set_handler(move || {
		if shutdown.load(Ordering::Acquire) {
			process::exit(0);
		}
		eprintln!("Shutdown requested. CTRL+C to force.");
		shutdown.store(true, Ordering::Release);
	})?;

	match select_socket_type_from_args() {
		Type::Tcp => try_tcp_main(port.as_str(), &handler_shutdown),
		Type::Udp => try_udp_main(port.as_str(), &handler_shutdown),
		Type::None => {
			eprintln!("No socket type selected. Available problems: tcp, udp");
			Ok(())
		},
	}
}

fn try_udp_main(port: &str, shutdown_flag: &Arc<AtomicBool>) -> Result<(), Error> {
	let problem: Arc<Box<dyn UdpHandler>> = Arc::new(match select_udp_problem_from_args() {
		UdpProblem::None => {
			eprintln!("No problem selected. Available problems: ");
			for &(key, _) in UDP_PROBLEMS.iter() {
				eprintln!("  - {}", key);
			}
			return Ok(());
		},
		UdpProblem::UnusualDatabaseProgram => Box::new(UnusualDatabaseProgram::new()),
	});

	let socket = UdpSocket::bind(format!("0.0.0.0:{port}")).map_err(Error::from)?;
	socket.set_nonblocking(true).expect("Failed to set nonblocking");
	eprintln!("Ready to accept UDP messages on {}", socket.local_addr()?);

	let wait_duration = Duration::from_millis(100);

	let mut handler_socket = socket.try_clone()?;

	loop {
		let mut buffer = [0; 1024];
		match socket.recv_from(&mut buffer) {
			Ok((size, addr)) => {
				let data = &buffer[0..size];
				eprintln!("({addr}) Data: '{}' ", data_to_hex(data));

				if let Err(e) = problem.handler(data, &mut handler_socket, addr) {
					eprintln!("{}", e);
				}
			},
			Err(ref err) if err.kind() == ErrorKind::WouldBlock => {
				if shutdown_flag.load(Ordering::Acquire) {
					problem.shutdown();
					break;
				}
				thread::sleep(wait_duration);
			},
			Err(err) => return Err(Error::from(err)),
		}
	}
	Ok(())
}

fn try_tcp_main(port: &str, shutdown_flag: &Arc<AtomicBool>) -> Result<(), Error> {
	let problem: Arc<Box<dyn TcpHandler>> = Arc::new(match select_tcp_problem_from_args() {
		TcpProblem::None => {
			eprintln!("No problem selected. Available problems: ");
			for &(key, _) in TCP_PROBLEMS.iter() {
				eprintln!("  - {}", key);
			}
			return Ok(());
		},
		TcpProblem::SmokeTest => Box::new(SmokeTest::new()),
		TcpProblem::PrimeTime => Box::new(PrimeTime::new()),
		TcpProblem::MeansToAnEnd => Box::new(MeansToAnEnd::new()),
		TcpProblem::BudgetChat => Box::new(BudgetChat::new()),
	});

	let number_workers = concurrency_from_environment()?;

	let listener = TcpListener::bind(format!("0.0.0.0:{port}")).map_err(Error::from)?;
	listener.set_nonblocking(true).expect("Failed to set nonblocking");
	eprintln!("Ready to accept TCP connections on {}", listener.local_addr()?);

	let pool = ThreadPool::new(number_workers);
	let mut connection_id: u32 = 0;

	let wait_duration = Duration::from_millis(100);

	loop {
		match listener.accept() {
			Ok((stream, addr)) => {
				connection_id = connection_id.wrapping_add(1);
				eprintln!("({connection_id}) Client connected: {addr}");
				let thread_problem = Arc::clone(&problem);
				pool.execute(move || {
					if let Err(e) = thread_problem.handler(stream, connection_id) {
						eprintln!("{}", e);
					}
				});
			},
			Err(ref err) if err.kind() == ErrorKind::WouldBlock => {
				if shutdown_flag.load(Ordering::Acquire) {
					problem.shutdown();
					break;
				}
				thread::sleep(wait_duration);
			},
			Err(err) => return Err(Error::from(err)),
		}
	}
	Ok(())
}

fn select_socket_type_from_args() -> Type {
	let socket_type = env::args().nth(1).unwrap_or_default().to_lowercase();

	match socket_type.as_str() {
		"tcp" => Type::Tcp,
		"udp" => Type::Udp,
		_ => Type::None,
	}
}

fn select_udp_problem_from_args() -> UdpProblem {
	let mut problems = HashMap::from(*UDP_PROBLEMS);
	problems
		.remove(env::args().nth(2).unwrap_or_default().to_lowercase().as_str())
		.unwrap_or(UdpProblem::None)
}

fn select_tcp_problem_from_args() -> TcpProblem {
	let mut problems = HashMap::from(*TCP_PROBLEMS);
	problems
		.remove(
			env::args()
				.nth(2)
				.unwrap_or_default()
				.to_lowercase()
				.replace('_', "")
				.as_str(),
		)
		.unwrap_or(TcpProblem::None)
}

fn concurrency_from_environment() -> Result<NonZeroUsize, Error> {
	let concurrency = env::var("CONCURRENCY")
		.unwrap_or_else(|_| String::from("10"))
		.parse::<usize>()
		.map_err(|_e| anyhow!("Environment variable CONCURRENCY must be a positive integer"))?;

	if concurrency < 1 {
		return Err(anyhow!("Environment variable CONCURRENCY must be a positive integer"));
	}

	Ok(NonZeroUsize::new(concurrency).unwrap())
}
