// std
use std::env;
// crates.io
use anyhow::Result;
use clap::{
	builder::{
		styling::{AnsiColor, Effects},
		Styles,
	},
	Parser,
};
use reqwew::{
	reqwest::{
		header::{HeaderMap, AUTHORIZATION, USER_AGENT},
		ClientBuilder,
	},
	Client, Http, Response,
};
use serde_json::Value;

/// Cli.
#[derive(Debug, Parser)]
#[command(
	version = concat!(
		env!("CARGO_PKG_VERSION"),
		"-",
		env!("VERGEN_GIT_SHA"),
		"-",
		env!("VERGEN_CARGO_TARGET_TRIPLE"),
	),
	about,
	rename_all = "kebab",
	styles = styles(),
)]
pub struct Cli {
	/// Repository to perform auto merge.
	#[arg(value_name = "NAME")]
	repository: String,
}
impl Cli {
	pub async fn run(&self) -> Result<()> {
		let Cli { repository } = self;
		let gh_tk = env::var("GITHUB_TOKEN")?;
		let c = C::new(&gh_tk)?;

		for (number, sha, title) in c.list_pull_requests(repository).await? {
			println!("checking: {title}");

			if c.check_pull_request(repository, &sha).await? {
				println!("merging: {title}");

				c.merge_pull_request(repository, number).await?;
			}
		}

		Ok(())
	}
}

fn styles() -> Styles {
	Styles::styled()
		.header(AnsiColor::Red.on_default() | Effects::BOLD)
		.usage(AnsiColor::Red.on_default() | Effects::BOLD)
		.literal(AnsiColor::Blue.on_default() | Effects::BOLD)
		.placeholder(AnsiColor::Green.on_default())
}

#[derive(Debug)]
struct C(Client);
impl C {
	fn new(github_token: &str) -> Result<Self> {
		let c = ClientBuilder::new()
			.default_headers(HeaderMap::from_iter([
				(AUTHORIZATION, format!("token {github_token}").parse()?),
				(USER_AGENT, "auto-merge".parse()?),
			]))
			.build()?;
		let c = Client::from(c);

		Ok(Self(c))
	}

	// List pull requests of the repository.
	async fn list_pull_requests(&self, repository: &str) -> Result<Vec<(u32, String, String)>> {
		let uri = format!("https://api.github.com/repos/{repository}/pulls");
		let res = self.0.get_with_retries(&uri, 3, 200).await?.json::<Value>()?;
		// Filter the author `dependabot[bot]`.
		let prs = res
			.as_array()
			.ok_or_else(|| anyhow::anyhow!("unexpected response: {res}"))?
			.iter()
			.filter_map(|pr| {
				let user = pr["user"]["login"].as_str()?;

				if user == "dependabot[bot]" {
					let pr_number = pr["number"].as_u64()?;
					let sha = pr["head"]["sha"].as_str()?;

					pr["head"]["ref"]
						.as_str()
						.map(|pr_ref| (pr_number as _, sha.into(), pr_ref.into()))
				} else {
					None
				}
			})
			.collect();

		Ok(prs)
	}

	// Check if the pull request's checks are passed.
	async fn check_pull_request(&self, repository: &str, sha: &str) -> Result<bool> {
		let uri = format!("https://api.github.com/repos/{repository}/commits/{sha}/check-runs");
		let res = self.0.get_with_retries(&uri, 3, 200).await?.json::<Value>()?;
		let checks = res["check_runs"]
			.as_array()
			.ok_or_else(|| anyhow::anyhow!("unexpected response: {res}"))?;
		let checks_passed = checks.iter().all(|check| {
			let name = check["name"].as_str().unwrap_or_default();
			let status = check["status"].as_str().unwrap_or_default();
			let conclusion = check["conclusion"].as_str().unwrap_or_default();

			println!("  name: {name}, status: {status}, conclusion: {conclusion}");

			status == "completed" && conclusion == "success"
		});

		Ok(checks_passed)
	}

	// Squash pull request of the repository.
	async fn merge_pull_request(&self, repository: &str, number: u32) -> Result<()> {
		let uri = format!("https://api.github.com/repos/{repository}/pulls/{number}/merge");
		let body = r#"{"merge_method":"squash"}"#;
		let res = self.0.put_with_retries(&uri, body, 3, 200).await?.json::<Value>()?;
		let merged = res["merged"].as_bool().unwrap_or_default();
		let message = res["message"].as_str().unwrap_or_default();

		println!("  merged: {merged}, message: {message}");

		Ok(())
	}
}
