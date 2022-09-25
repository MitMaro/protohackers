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
	unused_results,
	variant_size_differences
)]
// enable all of Clippy's lints
#![deny(clippy::all, clippy::cargo, clippy::pedantic, clippy::restriction)]
#![allow(
	clippy::blanket_clippy_restriction_lints,
	clippy::default_numeric_fallback,
	clippy::else_if_without_else,
	clippy::expect_used,
	clippy::implicit_return,
	clippy::integer_arithmetic,
	clippy::missing_docs_in_private_items,
	clippy::mod_module_files,
	clippy::module_name_repetitions,
	clippy::option_if_let_else,
	clippy::pub_use,
	clippy::redundant_pub_crate,
	clippy::tabs_in_doc_comments,
	clippy::too_many_lines
)]

mod job;
mod prime_time;
mod smoke_test;
mod thread_pool;
mod worker;

use std::{collections::HashMap, env, net::TcpListener, num::NonZeroUsize};

use anyhow::{anyhow, Error};
use lazy_static::lazy_static;
use thread_pool::ThreadPool;

#[derive(Debug, Copy, Clone)]
enum Problem {
	None,
	SmokeTest,
	PrimeTime,
}
lazy_static! {
	static ref PROBLEMS: [(&'static str, Problem); 2] =
		[("smoketest", Problem::SmokeTest), ("primetime", Problem::PrimeTime)];
}

fn main() {
	if let Err(e) = try_main() {
		eprintln!("{}", e);
		std::process::exit(1);
	}
}

fn try_main() -> Result<(), Error> {
	let problem = select_problem_from_args();

	let handler = match problem {
		Problem::None => {
			eprintln!("No problem selected. Available problems: ");
			for (key, _) in PROBLEMS.iter() {
				eprintln!("  - {}", key);
			}
			return Ok(());
		},
		Problem::SmokeTest => smoke_test::handle,
		Problem::PrimeTime => prime_time::handle,
	};

	let port = env::var("PORT").unwrap_or(String::from("7878"));
	let number_workers = concurrency_from_environment()?;
	let address = format!("0.0.0.0:{port}");
	eprintln!("Starting TCP server on {}", address);

	let listener = TcpListener::bind(address).map_err(Error::from)?;
	let pool = ThreadPool::new(number_workers);
	let mut id = 0;

	for stream in listener.incoming() {
		id += 1;
		let stream = stream.unwrap();
		eprintln!("Steam Started: {}", id);

		pool.execute(move || {
			if let Err(e) = handler(stream, id) {
				eprintln!("{}", e);
			}
		});
	}

	Ok(())
}

fn select_problem_from_args() -> Problem {
	let mut problems = HashMap::from(*PROBLEMS);
	problems
		.remove(env::args().skip(1).next().unwrap_or(String::from("")).as_str())
		.unwrap_or(Problem::None)
}

fn concurrency_from_environment() -> Result<NonZeroUsize, Error> {
	let concurrency = env::var("CONCURRENCY")
		.unwrap_or(String::from("100"))
		.parse::<usize>()
		.map_err(|_e| anyhow!("Environment variable CONCURRENCY must be a positive integer"))?;

	if concurrency < 1 {
		return Err(anyhow!("Environment variable CONCURRENCY must be a positive integer"));
	}

	Ok(NonZeroUsize::new(concurrency).unwrap())
}
