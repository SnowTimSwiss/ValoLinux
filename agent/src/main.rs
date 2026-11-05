//! valo-agent: runs on each device. Enrolls with the server, shows the
//! adoption code, then regularly fetches the signed manifest and applies it.

mod apply;
mod client;
mod platform;

use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;
use valo_proto::{EnrollRequest, EnrollResponse, Identity, ManifestResponse, Report, SignedManifest};

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Parser)]
#[command(name = "valo-agent", version, about = "ValoLinux device agent")]
struct Cli {
    /// Where the device key and cached manifest live.
    #[arg(long, default_value = "/var/lib/valo-agent", global = true)]
    state_dir: PathBuf,
    /// System root to apply to. Anything but "/" implies --dry-run for commands.
    #[arg(long, default_value = "/", global = true)]
    root: PathBuf,
    /// Write files but only log commands instead of running them.
    #[arg(long, global = true)]
    dry_run: bool,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Register with a server and show the adoption code.
    Enroll {
        #[arg(long)]
        server: Option<String>,
    },
    /// Keep the device in sync (what the systemd service runs).
    Run {
        #[arg(long)]
        server: Option<String>,
        /// Seconds between checks once adopted.
        #[arg(long, default_value_t = 60)]
        interval: u64,
        /// Do a single check and exit.
        #[arg(long)]
        once: bool,
    },
    /// Show enrollment and sync state.
    Status,
    /// Forget server and identity (device must be adopted again).
    Reset,
}

/// Server binding, stored after the first successful enrollment.
#[derive(Serialize, Deserialize, Clone)]
struct ServerBinding {
    url: String,
    /// Pinned at enrollment; manifests signed with another key are refused.
    server_key: String,
}

struct Agent {
    state_dir: PathBuf,
    root: PathBuf,
    dry_run: bool,
    key: Identity,
}

fn read_json<T: for<'de> Deserialize<'de>>(p: &Path) -> Option<T> {
    serde_json::from_str(&std::fs::read_to_string(p).ok()?).ok()
}

fn write_json<T: Serialize>(p: &Path, v: &T) -> Result<(), String> {
    let tmp = p.with_extension("tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(v).map_err(|e| e.to_string())?)
        .and_then(|_| std::fs::rename(&tmp, p))
        .map_err(|e| format!("{}: {e}", p.display()))
}

fn load_or_create_key(dir: &Path) -> Result<Identity, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    restrict(dir, 0o700);
    let path = dir.join("device.key");
    if let Ok(seed) = std::fs::read(&path) {
        return Identity::from_seed(&seed).map_err(|e| e.to_string());
    }
    let id = Identity::generate().map_err(|e| e.to_string())?;
    std::fs::write(&path, id.seed()).map_err(|e| format!("{}: {e}", path.display()))?;
    restrict(&path, 0o600);
    Ok(id)
}

#[cfg(unix)]
fn restrict(p: &Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(p, std::fs::Permissions::from_mode(mode));
}

fn first_line(p: &Path) -> String {
    std::fs::read_to_string(p).map(|s| s.lines().next().unwrap_or("").trim().to_string()).unwrap_or_default()
}

fn os_name(root: &Path) -> String {
    std::fs::read_to_string(root.join("etc/os-release"))
        .ok()
        .and_then(|s| {
            s.lines()
                .find_map(|l| l.strip_prefix("PRETTY_NAME=").map(|v| v.trim_matches('"').to_string()))
        })
        .unwrap_or_else(|| "unbekannt".into())
}

fn hardware() -> String {
    let vendor = first_line(Path::new("/sys/class/dmi/id/sys_vendor"));
    let product = first_line(Path::new("/sys/class/dmi/id/product_name"));
    let mem = std::fs::read_to_string("/proc/meminfo")
        .ok()
        .and_then(|s| s.lines().next().map(|l| l.split_whitespace().nth(1).unwrap_or("0").parse::<u64>().unwrap_or(0)))
        .map(|kb| format!("{:.0} GB RAM", kb as f64 / 1024.0 / 1024.0))
        .unwrap_or_default();
    [format!("{vendor} {product}").trim().to_string(), mem]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(", ")
}

fn hostname() -> String {
    let h = first_line(Path::new("/etc/hostname"));
    if h.is_empty() { first_line(Path::new("/proc/sys/kernel/hostname")) } else { h }
}

/// Server URL from: --server, saved binding, /etc/valolinux/agent.conf, kernel cmdline `valo.server=`.
fn configured_server(root: &Path) -> Option<String> {
    let conf = std::fs::read_to_string(root.join("etc/valolinux/agent.conf")).unwrap_or_default();
    let from_conf = conf.lines().find_map(|l| {
        let l = l.trim();
        l.strip_prefix("server").map(|r| r.trim_start().trim_start_matches('=').trim().trim_matches('"').to_string())
    });
    from_conf.filter(|s| !s.is_empty()).or_else(|| {
        std::fs::read_to_string("/proc/cmdline")
            .ok()?
            .split_whitespace()
            .find_map(|a| a.strip_prefix("valo.server=").map(str::to_string))
    })
}

/// Well-known addresses tried when nothing is configured: a host named
/// "valolinux" via mDNS (.local) or the school's DNS.
const DISCOVERY_CANDIDATES: &[&str] = &["http://valolinux.local:8080", "http://valolinux:8080"];

/// Find a server that answers /healthz.
fn discover() -> Option<String> {
    let http: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(3)))
        .http_status_as_error(false)
        .build()
        .into();
    DISCOVERY_CANDIDATES.iter().find_map(|base| {
        let ok = http.get(format!("{base}/healthz")).call().map(|r| r.status().as_u16() == 200).unwrap_or(false);
        ok.then(|| base.to_string())
    })
}

fn format_code(code: &str) -> String {
    if code.len() == 6 { format!("{}-{}", &code[..3], &code[3..]) } else { code.to_string() }
}

impl Agent {
    fn binding(&self) -> Option<ServerBinding> {
        read_json(&self.state_dir.join("server.json"))
    }

    fn platform(&self) -> platform::Platform {
        platform::Platform::new(&self.root, self.dry_run)
    }

    fn enroll(&self, url: &str) -> Result<ServerBinding, String> {
        let c = client::Client::new(url, &self.key);
        let req = EnrollRequest {
            public_key: self.key.public_key(),
            hostname: hostname(),
            os: os_name(&self.root),
            agent_version: VERSION.into(),
            hardware: hardware(),
        };
        let r = c.post_json("/api/v1/enroll", &req)?;
        if r.status != 200 {
            return Err(format!("enroll failed ({}): {}", r.status, r.body));
        }
        let resp: EnrollResponse = serde_json::from_str(&r.body).map_err(|e| e.to_string())?;
        let binding = match self.binding() {
            // keep the pinned key; refuse a server that changed its key
            Some(b) if b.url == url => {
                if b.server_key != resp.server_key {
                    return Err("server key changed! refusing (run `valo-agent reset` if intended)".into());
                }
                b
            }
            _ => ServerBinding { url: url.to_string(), server_key: resp.server_key },
        };
        write_json(&self.state_dir.join("server.json"), &binding)?;
        self.show_code(Some(&resp.code), &binding.url);
        Ok(binding)
    }

    /// Show (or clear) the adoption code: console login banner + a file for the desktop.
    fn show_code(&self, code: Option<&str>, url: &str) {
        let mut p = self.platform();
        match code {
            Some(code) => {
                let c = format_code(code);
                println!("\n  Gerät hinzufügen: Code {c}\n  Verwaltung: {url}\n");
                let _ = p.write("/run/valolinux/adoption-code", &format!("{c}\n"), 0o644);
                let _ = p.write(
                    "/etc/issue.d/valolinux.issue",
                    &format!("\n  ValoLinux: Dieses Gerät ist noch nicht eingerichtet.\n  Code in der Verwaltung eingeben: {c}\n\n"),
                    0o644,
                );
            }
            None => {
                let _ = p.remove("/run/valolinux/adoption-code");
                let _ = p.remove("/etc/issue.d/valolinux.issue");
            }
        }
    }

    /// Human-readable state for the desktop helper (/run/valolinux/status).
    fn set_status(&self, text: &str) {
        let mut p = self.platform();
        let _ = if text.is_empty() {
            p.remove("/run/valolinux/status").map(|_| true)
        } else {
            p.write("/run/valolinux/status", &format!("{text}\n"), 0o644)
        };
    }

    fn applied(&self) -> apply::Applied {
        read_json(&self.state_dir.join("applied.json")).unwrap_or_default()
    }

    /// Apply the configuration (if `configure`), report, then run new actions.
    fn apply_and_report(&self, m: &valo_proto::Manifest, c: Option<&client::Client>, configure: bool) -> Result<(), String> {
        let mut state = self.applied();
        let mut p = self.platform();
        let out = if configure {
            apply::apply(&mut p, m, &mut state)
        } else {
            apply::Outcome { ok: state.ok, messages: vec![] }
        };
        // offline (no client): don't run actions from a possibly old cached manifest
        let actions = if c.is_some() { apply::take_new_actions(m, &mut state) } else { vec![] };
        write_json(&self.state_dir.join("applied.json"), &state)?;
        for line in &out.messages {
            println!("  {line}");
        }
        if self.dry_run || self.root != Path::new("/") {
            for l in &p.log {
                println!("  [dry-run] {l}");
            }
        }
        if let Some(c) = c {
            let report = Report {
                revision: Some(m.revision.clone()),
                ok: out.ok,
                messages: out.messages,
                agent_version: VERSION.into(),
                hostname: hostname(),
                os: os_name(&self.root),
                done_actions: actions.iter().map(|a| a.id).collect(),
            };
            let r = c.post_json("/api/v1/report", &report)?;
            if r.status >= 300 {
                return Err(format!("report failed ({})", r.status));
            }
        }
        for a in &actions {
            println!("Aktion: {}", a.kind);
            if let Err(e) = apply::run_action(&mut p, a) {
                eprintln!("{e}");
            }
            if self.dry_run || self.root != Path::new("/") {
                println!("  [dry-run] {}", p.log.last().cloned().unwrap_or_default());
            }
        }
        Ok(())
    }

    /// One sync cycle. Returns true if the device is adopted.
    fn cycle(&self, binding: &ServerBinding) -> Result<bool, String> {
        let c = client::Client::new(&binding.url, &self.key);
        let r = c.get("/api/v1/manifest")?;
        if r.status == 404 {
            // server forgot us (device removed): enroll again for a new code
            self.enroll(&binding.url)?;
            return Ok(false);
        }
        if r.status != 200 {
            return Err(format!("manifest request failed ({}): {}", r.status, r.body));
        }
        match serde_json::from_str::<ManifestResponse>(&r.body).map_err(|e| e.to_string())? {
            ManifestResponse::Pending { code } => {
                self.show_code(Some(&code), &binding.url);
                Ok(false)
            }
            ManifestResponse::Adopted { manifest } => {
                let m = manifest.open(&binding.server_key).map_err(|e| format!("manifest rejected: {e}"))?;
                self.show_code(None, &binding.url);
                write_json(&self.state_dir.join("manifest.json"), &manifest)?;
                let st = self.applied();
                let outdated = st.revision.as_deref() != Some(&m.revision) || !st.ok;
                let new_actions = m.actions.iter().any(|a| !st.done_actions.contains(&a.id));
                if outdated {
                    println!("Wende Konfiguration {} an ({}, {})", m.revision, m.device_name, m.role);
                }
                if outdated || new_actions {
                    self.apply_and_report(&m, Some(&c), outdated)?;
                }
                Ok(true)
            }
        }
    }

    /// Offline start: apply the last good manifest from cache.
    fn apply_cached(&self, binding: &ServerBinding) {
        let Some(signed) = read_json::<SignedManifest>(&self.state_dir.join("manifest.json")) else { return };
        match signed.open(&binding.server_key) {
            Ok(m) if self.applied().revision.as_deref() != Some(&m.revision) => {
                println!("Server nicht erreichbar, wende gespeicherte Konfiguration {} an", m.revision);
                let _ = self.apply_and_report(&m, None, true);
            }
            Ok(_) => {}
            Err(e) => eprintln!("cached manifest invalid: {e}"),
        }
    }
}

fn main() {
    let cli = Cli::parse();
    if let Err(e) = run(cli) {
        eprintln!("valo-agent: {e}");
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<(), String> {
    if matches!(cli.cmd, Cmd::Reset) {
        for f in ["device.key", "server.json", "manifest.json", "applied.json"] {
            let _ = std::fs::remove_file(cli.state_dir.join(f));
        }
        println!("Identität und Server vergessen. Beim nächsten Start gibt es einen neuen Code.");
        return Ok(());
    }
    let agent = Agent { key: load_or_create_key(&cli.state_dir)?, state_dir: cli.state_dir, root: cli.root, dry_run: cli.dry_run };
    match cli.cmd {
        Cmd::Enroll { server } => {
            let url = server
                .or_else(|| agent.binding().map(|b| b.url))
                .or_else(|| configured_server(&agent.root))
                .ok_or("no server configured (use --server URL)")?;
            agent.enroll(&url)?;
        }
        Cmd::Status => {
            println!("Gerät:    {}", agent.key.public_key());
            match agent.binding() {
                Some(b) => println!("Server:   {} (Schlüssel {}…)", b.url, &b.server_key[..12]),
                None => println!("Server:   noch nicht angemeldet"),
            }
            let st = agent.applied();
            println!(
                "Stand:    {} ({})",
                st.revision.as_deref().unwrap_or("noch nie angewendet"),
                if st.ok { "ok" } else { "mit Fehlern" }
            );
            println!("Apps:     {}", if st.managed_apps.is_empty() { "-".into() } else { st.managed_apps.join(", ") });
        }
        Cmd::Run { server, interval, once } => {
            let explicit = server.or_else(|| agent.binding().map(|b| b.url)).or_else(|| configured_server(&agent.root));
            let url = match explicit {
                Some(u) => u,
                None => loop {
                    if let Some(u) = discover() {
                        println!("Server gefunden: {u}");
                        break u;
                    }
                    agent.set_status("Kein Verwaltungsserver gefunden. Server-Adresse in /etc/valolinux/agent.conf eintragen oder mit valo.server=http://… starten.");
                    if once {
                        return Err("no server configured or found".into());
                    }
                    std::thread::sleep(Duration::from_secs(30));
                },
            };
            agent.set_status("");
            let mut binding = agent.binding().filter(|b| b.url == url);
            if let Some(b) = &binding {
                agent.apply_cached(b);
            }
            loop {
                if binding.is_none() {
                    match agent.enroll(&url) {
                        Ok(b) => binding = Some(b),
                        Err(e) => eprintln!("{e}"),
                    }
                }
                let adopted = match &binding {
                    Some(b) => agent.cycle(b).unwrap_or_else(|e| {
                        eprintln!("{e}");
                        false
                    }),
                    None => false,
                };
                if once {
                    break;
                }
                // poll quickly while waiting for adoption, slowly afterwards
                std::thread::sleep(Duration::from_secs(if adopted { interval } else { 10 }));
            }
        }
        Cmd::Reset => unreachable!(),
    }
    Ok(())
}
