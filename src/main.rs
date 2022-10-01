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
mod utils;
mod worker;

use std::{
	collections::HashMap,
	env,
	io::ErrorKind,
	net::TcpListener,
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
	handler::Handler,
	means_to_an_end::MeansToAnEnd,
	prime_time::PrimeTime,
	smoke_test::SmokeTest,
};

#[derive(Debug, Copy, Clone)]
enum Problem {
	None,
	SmokeTest,
	PrimeTime,
	MeansToAnEnd,
	BudgetChat,
}

lazy_static! {
	static ref PROBLEMS: [(&'static str, Problem); 4] = [
		("smoketest", Problem::SmokeTest),
		("primetime", Problem::PrimeTime),
		("meanstoanend", Problem::MeansToAnEnd),
		("budgetchat", Problem::BudgetChat),
	];
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
	let problem: Arc<Box<dyn Handler>> = Arc::new(match select_problem_from_args() {
		Problem::None => {
			eprintln!("No problem selected. Available problems: ");
			for &(key, _) in PROBLEMS.iter() {
				eprintln!("  - {}", key);
			}
			return Ok(());
		},
		Problem::SmokeTest => Box::new(SmokeTest::new()),
		Problem::PrimeTime => Box::new(PrimeTime::new()),
		Problem::MeansToAnEnd => Box::new(MeansToAnEnd::new()),
		Problem::BudgetChat => Box::new(BudgetChat::new()),
	});

	let port = env::var("PORT").unwrap_or_else(|_| String::from("7878"));
	let number_workers = concurrency_from_environment()?;

	let listener = TcpListener::bind(format!("0.0.0.0:{port}")).map_err(Error::from)?;
	listener.set_nonblocking(true).expect("Failed to set nonblocking");
	eprintln!("Ready to accept TCP connections on {}", listener.local_addr()?);

	let pool = ThreadPool::new(number_workers);
	let mut connection_id: u32 = 0;

	let wait_duration = Duration::from_millis(100);

	let shutdown = Arc::new(AtomicBool::new(false));
	let shutdown_reader = Arc::clone(&shutdown);

	set_handler(move || {
		if shutdown.load(Ordering::Acquire) {
			process::exit(0);
		}
		eprintln!("Shutdown requested. CTRL+C to force.");
		shutdown.store(true, Ordering::Release);
	})?;

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
				if shutdown_reader.load(Ordering::Acquire) {
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

fn select_problem_from_args() -> Problem {
	let mut problems = HashMap::from(*PROBLEMS);
	problems
		.remove(
			env::args()
				.nth(1)
				.unwrap_or_default()
				.to_lowercase()
				.replace('_', "")
				.as_str(),
		)
		.unwrap_or(Problem::None)
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
