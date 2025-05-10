use config::Config;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::io::Write;
use std::net::Ipv4Addr;
use std::process::exit;
use std::str::FromStr;
use ureq::Agent;
use ureq::config::IpFamily::Ipv4Only;

const DEFAULT_IPS: [&str; 2] = ["https://ipv4.icanhazip.com", "https://api.ipify.org"];

#[derive(Debug, Serialize, Deserialize)]
struct CloudflareDDNS {
	ip_src: Option<Vec<String>>,
	auth_key: String,
	auth_email: String,
	zone_id: String,
	patterns: Option<Vec<String>>,
	invert_patterns: Option<bool>,
	http_timeout_s: Option<u64>,
}

impl Default for CloudflareDDNS {
	fn default() -> Self {
		Self {
			ip_src: Some(DEFAULT_IPS.into_iter().map(String::from).collect()),
			auth_key: Default::default(),
			auth_email: Default::default(),
			zone_id: Default::default(),
			patterns: None,
			invert_patterns: None,
			http_timeout_s: Some(10),
		}
	}
}

#[derive(Debug, Default, Deserialize)]
struct CloudflareDnsResponse<T> {
	success: bool,
	#[serde(rename = "result")]
	entries: Vec<T>,
	errors: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
struct CloudflareDnsRecord {
	id: String,
	r#type: String,
	name: String,
	#[serde(rename = "content")]
	ip: Ipv4Addr,
}

fn main() {
	let cddns = CloudflareDDNS::get_from_config();
	cddns.run();
}

impl CloudflareDDNS {
	fn get_from_config() -> Self {
		let conf: CloudflareDDNS;
		let conf_dir = dirs::config_dir().unwrap().join(env!("CARGO_PKG_NAME"));
		let conf_path = conf_dir.join("config.toml");

		if !conf_path.exists() {
			if let Err(e) = std::fs::create_dir_all(&conf_dir) {
				println!("couldn't create config dir: {e}");
			} else {
				match std::fs::File::create(&conf_path) {
					Ok(mut f) => {
						conf = CloudflareDDNS::default();
						let data = toml::to_string_pretty(&conf).unwrap();
						if let Err(e) = f.write_all(data.as_bytes()) {
							println!("couldn't write default config file: {e}");
						} else {
							println!(
								"default config file created at {conf_path:?}\nrequired fields: auth_key, auth_email, zone_id"
							);
						}
					}
					Err(e) => {
						println!("couldn't create config.toml: {e}");
					}
				}
			}
			exit(1);
		}

		match Config::builder()
			.set_default("ip_src", DEFAULT_IPS.into_iter().map(String::from).collect::<Vec<_>>())
			.unwrap()
			.set_default("http_timeout_s", 10)
			.unwrap()
			.add_source(config::File::with_name(conf_path.to_str().unwrap()))
			.add_source(config::Environment::with_prefix("CF"))
			.build()
		{
			Ok(c) => {
				match c.try_deserialize::<CloudflareDDNS>() {
					Ok(c) => conf = c,
					Err(e) => {
						println!("config error: {e:?}");
						exit(1);
					}
				};
			}
			Err(e) => {
				println!("couldn't parse config: {e}");
				exit(1);
			}
		}

		conf.check_missing();

		conf
	}

	fn check_missing(&self) {
		let mut missing = Vec::<&str>::new();
		if self.auth_key.is_empty() {
			missing.push("auth_key");
		}
		if self.auth_email.is_empty() {
			missing.push("auth_email");
		}
		if self.zone_id.is_empty() {
			missing.push("zone_id");
		}
		if !missing.is_empty() {
			println!("missing configuration entries: {}", missing.join(", "));
			exit(1);
		}
	}

	fn get_client(&self) -> Agent {
		Agent::config_builder()
			.user_agent(concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION")))
			.timeout_global(Some(std::time::Duration::from_secs(self.http_timeout_s.unwrap())))
			.https_only(true)
			.ip_family(Ipv4Only)
			.build()
			.into()
	}

	fn get_current_ipv4(&self, client: &Agent) -> Ipv4Addr {
		self.ip_src
			.as_ref()
			.unwrap()
			.iter()
			.find_map(|ip| {
				print!("trying {ip}: ");

				match client.get(ip).call() {
					Ok(resp) => match resp.into_body().read_to_string() {
						Ok(body) => match Ipv4Addr::from_str(body.trim()) {
							Ok(ip_addr) => Some(ip_addr),
							Err(e) => {
								println!("failed: {e}");
								None
							}
						},
						Err(e) => {
							println!("failed: {e}");
							None
						}
					},
					Err(e) => {
						println!("failed: {e}");
						None
					}
				}
			})
			.unwrap_or_else(|| {
				println!("could not determine external ip address");
				exit(1);
			})
	}

	fn run(self) {
		let client = self.get_client();

		println!("> getting external ipv4 address...");
		let current_ip = self.get_current_ipv4(&client);

		print!("{current_ip:?}\n> listing dns A-records... ");
		let a_records = self.get_a_records(&client);

		println!("\n> patching...",);
		self.patch_records(&client, &current_ip, a_records);

		println!("finished");
	}

	fn get_a_records(&self, client: &Agent) -> Vec<CloudflareDnsRecord> {
		match client
			.get(format!(
				"https://api.cloudflare.com/client/v4/zones/{}/dns_records?type=A",
				self.zone_id
			))
			.header("X-Auth-Email", &self.auth_email)
			.header("Authorization", format!("Bearer {}", self.auth_key))
			.call()
		{
			Ok(resp) => match resp.into_body().read_json::<CloudflareDnsResponse<CloudflareDnsRecord>>() {
				Ok(resp) => {
					if !resp.success {
						println!("cloudflare api error(s):\n{}", resp.errors.join("\n"));
						exit(1);
					}
					let mut a_records = resp
						.entries
						.iter()
						.filter(|x| x.r#type == "A")
						.map(|x| x.to_owned())
						.collect::<Vec<_>>();

					let total_records = a_records.len();
					if total_records == 0 {
						println!("none found");
						exit(0);
					}

					if let Some(patterns) = self.patterns.as_ref() {
						let matchers = patterns
							.iter()
							.map(|p| globset::Glob::new(p).expect("invalid pattern").compile_matcher())
							.collect::<Vec<_>>();

						a_records.retain(|x| {
							let matched = matchers.iter().any(|m| m.is_match(&x.name));
							if self.invert_patterns.unwrap_or(true) {
								!matched
							} else {
								matched
							}
						});
					}

					let filtered_records = a_records.len();
					if filtered_records == 0 {
						println!("all records were filtered");
						exit(0);
					}

					print!("{} found", total_records);
					if total_records > filtered_records {
						print!(", {} filtered", total_records - filtered_records);
					}

					return a_records;
				}
				Err(e) => {
					println!("failed:\n{e}");
					exit(1);
				}
			},
			Err(e) => {
				println!("failed:\n{e}");
				exit(1);
			}
		}
	}

	fn patch_records(&self, client: &Agent, current_ip: &Ipv4Addr, a_records: Vec<CloudflareDnsRecord>) {
		let mut errors = false;

		for (i, record) in a_records.into_iter().enumerate() {
			if record.ip == *current_ip {
				println!("record {} ({}): up to date", i + 1, record.name);
				continue;
			}
			print!("record {}: ", i + 1);

			match client
				.patch(format!(
					"https://api.cloudflare.com/client/v4/zones/{}/dns_records/{}",
					self.zone_id, record.id
				))
				.header("X-Auth-Email", &self.auth_email)
				.header("Authorization", format!("Bearer {}", self.auth_key))
				.send_json(json!({"content": current_ip.to_string()}))
			{
				Ok(resp) => {
					if resp.status().is_success() {
						println!("success");
					} else {
						let status = resp.status();
						println!("failed (http {})", status);

						let data = resp.into_body().read_json::<CloudflareDnsResponse<()>>().unwrap_or_default();
						if !data.errors.is_empty() {
							println!("error(s):\n{}", data.errors.join("\n"));
						}
					}
				}
				Err(e) => {
					println!("failed: {e}");
					errors = true;
				}
			}
		}

		if errors {
			println!("finished with errors");
			exit(1);
		}
	}
}
