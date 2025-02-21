use config::Config;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::io::Write;
use std::net::Ipv4Addr;
use std::process::exit;
use std::str::FromStr;
use ureq::config::IpFamily::Ipv4Only;

#[derive(Debug, Serialize, Deserialize)]
struct CloudflareDDNS {
	ip_src: Vec<String>,
	auth_key: String,
	auth_email: String,
	zone_id: String,
	http_timeout_s: Option<u64>,
}

impl Default for CloudflareDDNS {
	fn default() -> Self {
		Self {
			ip_src: vec![String::from("https://ipv4.icanhazip.com/"), String::from("https://api.ipify.org")],
			auth_key: String::new(),
			auth_email: String::new(),
			zone_id: String::new(),
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

#[derive(Debug, Deserialize)]
struct CloudflareDnsRecord {
	id: String,
	r#type: String,
	#[serde(rename = "content")]
	ip: Ipv4Addr,
}

fn main() {
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
						println!("default config file created at {conf_path:?}\nrequired fields: auth_key, auth_email, zone_id");
					}
 				},
				Err(e) => {
					println!("couldn't create config.toml: {e}");
				},
			}
		}
		exit(1);
	}

	match Config::builder()
		.set_default("ip_src", vec![String::from("https://ipv4.icanhazip.com/"), String::from("https://api.ipify.org")]).unwrap()
		.set_default("http_timeout_s", Some(10)).unwrap()
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
				},
			};
		},
		Err(e) => {
			println!("couldn't parse config: {e}");
			exit(1);
		},
	}

	/* detect missing config entries */ {
		let mut missing = Vec::<&str>::new();
		if conf.auth_key.is_empty() { missing.push("auth_key"); }
		if conf.auth_email.is_empty() { missing.push("auth_email"); }
		if conf.zone_id.is_empty() { missing.push("zone_id"); }
		if !missing.is_empty() {
			println!("missing configuration entries: {}", missing.join(", "));
			exit(1);
		}
	}

	let client: ureq::Agent = ureq::Agent::config_builder()
		.user_agent(concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION")))
		.timeout_global(Some(std::time::Duration::from_secs(conf.http_timeout_s.unwrap())))
		.https_only(true)
		.ip_family(Ipv4Only)
		.build()
		.into();

	println!("> getting external ipv4 address...");

	let ipv4 = conf.ip_src.iter().find_map(|ip| {
		print!("trying {ip}: ");

		match client.get(ip).call() {
			Ok(resp) => {
				match resp.into_body().read_to_string() {
					Ok(body) => {
						match Ipv4Addr::from_str(body.trim()) {
							Ok(ip_addr) => Some(ip_addr),
							Err(e) => {
								println!("failed: {e}");
								None
							},
						}
					},
					Err(e) => {
						println!("failed: {e}");
						None
					},
				}
			},
			Err(e) => {
				println!("failed: {e}");
				None
			},
		}
	}).unwrap_or_else(|| {
		println!("could not determine external ip address");
		exit(1);
	});

	print!("{ipv4:?}\n> listing dns A records... ");

	match client.get(format!("https://api.cloudflare.com/client/v4/zones/{}/dns_records?type=A", conf.zone_id))
		.header("X-Auth-Email", &conf.auth_email)
		.header("Authorization", format!("Bearer {}", conf.auth_key))
		.call() {
		Ok(resp) => {
			match resp.into_body().read_json::<CloudflareDnsResponse<CloudflareDnsRecord>>() {
				Ok(resp) => {
					if !resp.success {
						println!("cloudflare api error(s):\n{}", resp.errors.join("\n"));
						exit(1);
					}

					let mut a_records = vec![];

					for entry in resp.entries {
						if entry.r#type == "A" {
							a_records.push(entry);
						}
					}

					if a_records.is_empty() {
						println!("none found");
						exit(0);
					}

					println!("{} found\n> patching...", a_records.len());
					let mut errors = false;

					for (i, record) in a_records.into_iter().enumerate() {
						if record.ip == ipv4 {
							println!("record {}: up to date", i + 1);
							continue;
						}
						print!("record {}: ", i + 1);

						match client.patch(format!("https://api.cloudflare.com/client/v4/zones/{}/dns_records/{}", conf.zone_id, record.id))
							.header("X-Auth-Email", &conf.auth_email)
							.header("Authorization", format!("Bearer {}", conf.auth_key))
							.send_json(json!({"content": ipv4.to_string()})) {
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
							},
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
				},
				Err(e) => {
					println!("failed:\n{e}");
					exit(1);
				},
			}
		},
		Err(e) => {
			println!("failed:\n{e}");
			exit(1);
		},
	}

	println!("finished");
}
