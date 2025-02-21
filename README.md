# Cloudflare DDNS updater

Tiny applet to update Cloudflare DNS A records for your domain.

It is recommended to use a service to restart the program on a regular interval, for example `systemd` on Linux or `Task Scheduler` on Windows.

---

## Config

The configuration is stored using the `.toml` format in the OS-specific config directory.

### Location

| OS | Path |
| --- | --- |
| Linux | `$HOME/.config/cloudflare-ddns-updater/config.toml` |
| Windows | `%APPDATA%/cloudflare-ddns-updater/config.toml` |

### Entries

| Key | Type | Required | Description | Default |
| --- | --- | --- | --- | --- |
| `auth_key` | `String` | **yes** | Cloudflare authentication key<br>*My Profile > API Tokens > Create Token > Edit zone DNS (template)* | none |
| `auth_email` | `String` | **yes** | Cloudflare account Email | none |
| `zone_id` | `String` | **yes** | Cloudflare zone ID<br>*Websites > (Your website) > Overview > API (bottom right) > Zone ID* | none |
| `ip_src` | `Vec<String>` | no | List of URLs for fetching the external IPv4 | `["https://ipv4.icanhazip.com/",`<br>`"https://api.ipify.org"]` |
| `http_timeout_s` | `u64` | no | Timeout for all HTTP requests in seconds | 10 |
