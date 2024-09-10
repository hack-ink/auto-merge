//! Merge GitHub Actions pull request automatically.

#![deny(clippy::all, missing_docs, unused_crate_dependencies)]

mod cli;

mod prelude {
	pub use anyhow::Result;
}

#[tokio::main]
async fn main() -> prelude::Result<()> {
	color_eyre::install().unwrap();

	<cli::Cli as clap::Parser>::parse().run().await
}
