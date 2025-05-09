# Cloudflare DDNS updater

Tiny applet to update Cloudflare DNS A records for your domain.

It is recommended to use a service to restart the program on a regular interval, for example `systemd` on Linux or `Task Scheduler` on Windows.

---

## Config

The configuration is stored using the `.toml` format in the OS-specific config directory.

### Location

| OS       | Path                                                |
|----------|-----------------------------------------------------|
| Linux    | `$HOME/.config/cloudflare-ddns-updater/config.toml` |
| Windows  | `%APPDATA%/cloudflare-ddns-updater/config.toml`     |

### Entries

| Key               | Type          | Required | Description                                                                                          | Default                                 |
|-------------------|---------------|----------|------------------------------------------------------------------------------------------------------|-----------------------------------------|
| `auth_key`        | `String`      | **yes**  | Cloudflare authentication key<br>*My Profile > API Tokens > Create Token > Edit zone DNS (template)* |                                         |
| `auth_email`      | `String`      | **yes**  | Cloudflare account Email                                                                             |                                         |
| `zone_id`         | `String`      | **yes**  | Cloudflare zone ID<br>*Websites > (Your website) > Overview > API (bottom right) > Zone ID*          |                                         |
| `patterns`        | `Vec<String>` |          | List of glob-patterns to match the DNS record name with                                              |                                         |
| `invert_patterns` | `bool`        |          | Inverts the effect of patterns                                                                       |                                         |
| `ip_src`          | `Vec<String>` |          | List of URLs used for fetching the external IPv4                                                     | `ipv4.icanhazip.com`<br>`api.ipify.org` |
| `http_timeout_s`  | `u64`         |          | Timeout for all HTTP requests in seconds                                                             | 10                                      |
