// src/main.rs
use chrono::Local;
use std::fmt;
use sysinfo::{System, Process};
use std::io::{BufRead, BufReader, Write, stdin};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use std::fs::OpenOptions;
use std::env;

const AUTH_TOKEN: &str = "ENSPD2026";
const PORT: u16 = 7878;

// ─── Étape 1 : Types métier ───────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct CpuInfo {
    usage_percent: f32,
    core_count: usize,
}

#[derive(Debug, Clone)]
struct MemInfo {
    total_mb: u64,
    used_mb: u64,
    free_mb: u64,
}

#[derive(Debug, Clone)]
struct ProcessInfo {
    pid: u32,
    name: String,
    cpu_usage: f32,
    memory_mb: u64,
}

#[derive(Debug, Clone)]
struct SystemSnapshot {
    timestamp: String,
    cpu: CpuInfo,
    memory: MemInfo,
    top_processes: Vec<ProcessInfo>,
}

// ─── Trait Display ────────────────────────────────────────────────────────────

impl fmt::Display for CpuInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "CPU: {:.1}% ({} cœurs)", self.usage_percent, self.core_count)
    }
}

impl fmt::Display for MemInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "MEM: {}MB utilisés / {}MB total ({} MB libres)",
            self.used_mb, self.total_mb, self.free_mb
        )
    }
}

impl fmt::Display for ProcessInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "  [{:>6}] {:<25} CPU:{:>5.1}%  MEM:{:>5}MB",
            self.pid, self.name, self.cpu_usage, self.memory_mb
        )
    }
}

impl fmt::Display for SystemSnapshot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "=== SysWatch — {} ===", self.timestamp)?;
        writeln!(f, "{}", self.cpu)?;
        writeln!(f, "{}", self.memory)?;
        writeln!(f, "--- Top Processus ---")?;
        for p in &self.top_processes {
            writeln!(f, "{}", p)?;
        }
        write!(f, "=====================")
    }
}

// ─── Étape 2 : Gestion d'erreurs ─────────────────────────────────────────────

#[derive(Debug)]
enum SysWatchError {
    CollectionFailed(String),
}

impl fmt::Display for SysWatchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SysWatchError::CollectionFailed(msg) => write!(f, "Erreur collecte: {}", msg),
        }
    }
}

impl std::error::Error for SysWatchError {}

fn collect_snapshot() -> Result<SystemSnapshot, SysWatchError> {
    let mut sys = System::new_all();
    sys.refresh_all();

    std::thread::sleep(std::time::Duration::from_millis(500));
    sys.refresh_all();

    let cpu_usage = sys.global_cpu_info().cpu_usage();
    let core_count = sys.cpus().len();

    if core_count == 0 {
        return Err(SysWatchError::CollectionFailed("Aucun CPU détecté".to_string()));
    }

    let total_mb = sys.total_memory() / 1024 / 1024;
    let used_mb  = sys.used_memory()  / 1024 / 1024;
    let free_mb  = sys.free_memory()  / 1024 / 1024;

    let mut processes: Vec<ProcessInfo> = sys
        .processes()
        .values()
        .map(|p: &Process| ProcessInfo {
            pid:       p.pid().as_u32(),
            name:      p.name().to_string(),
            cpu_usage: p.cpu_usage(),
            memory_mb: p.memory() / 1024 / 1024,
        })
        .collect();

    processes.sort_by(|a, b| b.cpu_usage.partial_cmp(&a.cpu_usage).unwrap());
    processes.truncate(5);

    Ok(SystemSnapshot {
        timestamp:     Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        cpu:           CpuInfo { usage_percent: cpu_usage, core_count },
        memory:        MemInfo { total_mb, used_mb, free_mb },
        top_processes: processes,
    })
}

// ─── Étape 3 : Formatage des réponses ────────────────────────────────────────

fn format_response(snapshot: &SystemSnapshot, command: &str) -> String {
    let cmd = command.trim().to_lowercase();

    match cmd.as_str() {
        "cpu" => {
            let bar: String = (0..10)
                .map(|i| if i < (snapshot.cpu.usage_percent / 10.0) as usize { '█' } else { '░' })
                .collect();
            format!("[CPU]\n{}\n[{}] {:.1}%\n", snapshot.cpu, bar, snapshot.cpu.usage_percent)
        }

        "mem" => {
            let percent = snapshot.memory.used_mb as f64 / snapshot.memory.total_mb as f64 * 100.0;
            let bar: String = (0..20)
                .map(|i| if i < (percent / 5.0) as usize { '█' } else { '░' })
                .collect();
            format!("[MÉMOIRE]\n{}\n[{}] {:.1}%\n", snapshot.memory, bar, percent)
        }

        "ps" | "procs" => {
            let lines: String = snapshot
                .top_processes
                .iter()
                .enumerate()
                .map(|(i, p)| format!("{}. {}", i + 1, p))
                .collect::<Vec<_>>()
                .join("\n");
            format!("[PROCESSUS — Top {}]\n{}\n", snapshot.top_processes.len(), lines)
        }

        "all" | "" => format!("{}\n", snapshot),

        "help" => concat!(
            "Commandes disponibles:\n",
            "  cpu   — Usage CPU\n",
            "  mem   — Mémoire RAM\n",
            "  ps    — Top 5 processus\n",
            "  all   — Vue complète\n",
            "  help  — Cette aide\n",
            "  quit  — Fermer la connexion\n",
        ).to_string(),

        "quit" | "exit" => "BYE\n".to_string(),

        _ => format!("Commande inconnue: '{}'. Tape 'help'.\n", command.trim()),
    }
}

// ─── Étape 5 : Journalisation ─────────────────────────────────────────────────

fn log_event(message: &str) {
    let line = format!("[{}] {}\n", Local::now().format("%Y-%m-%d %H:%M:%S"), message);
    print!("{}", line);
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open("syswatch.log") {
        let _ = file.write_all(line.as_bytes());
    }
}

// ─── Étape 4 : Gestion d'un client (agent) ───────────────────────────────────

fn handle_client(mut stream: TcpStream, snapshot: Arc<Mutex<SystemSnapshot>>) {
    let peer = stream.peer_addr()
        .map(|a| a.to_string())
        .unwrap_or_else(|_| "inconnu".to_string());

    log_event(&format!("[+] Connexion de {}", peer));

    let _ = stream.write_all(b"TOKEN: ");

    let mut reader = BufReader::new(stream.try_clone().expect("Clonage du stream échoué"));
    let mut token_line = String::new();

    if reader.read_line(&mut token_line).is_err() || token_line.trim() != AUTH_TOKEN {
        let _ = stream.write_all(b"UNAUTHORIZED\n");
        log_event(&format!("[!] Accès refusé depuis {}", peer));
        return;
    }

    let _ = stream.write_all(b"OK\n");
    log_event(&format!("[✓] Authentifié: {}", peer));

    for line in reader.lines() {
        match line {
            Ok(cmd) => {
                let cmd = cmd.trim().to_string();
                log_event(&format!("[{}] commande: '{}'", peer, cmd));

                if cmd.eq_ignore_ascii_case("quit") {
                    let _ = stream.write_all(b"BYE\n");
                    break;
                }

                let response = {
                    let snap = snapshot.lock().unwrap();
                    format_response(&snap, &cmd)
                };

                let _ = stream.write_all(response.as_bytes());
                let _ = stream.write_all(b"\nEND\n");
            }
            Err(_) => break,
        }
    }

    log_event(&format!("[-] Déconnexion de {}", peer));
}

fn snapshot_refresher(snapshot: Arc<Mutex<SystemSnapshot>>) {
    loop {
        thread::sleep(Duration::from_secs(5));
        match collect_snapshot() {
            Ok(new_snap) => {
                let mut snap = snapshot.lock().unwrap();
                *snap = new_snap;
                println!("[refresh] Métriques mises à jour");
            }
            Err(e) => eprintln!("[refresh] Erreur: {}", e),
        }
    }
}

// ─── Mode AGENT (serveur) ─────────────────────────────────────────────────────

fn run_agent() {
    println!("SysWatch — mode AGENT");

    let initial = collect_snapshot().expect("Collecte initiale impossible");
    println!("Métriques initiales:\n{}", initial);

    let shared = Arc::new(Mutex::new(initial));

    let snap_clone = Arc::clone(&shared);
    thread::spawn(move || snapshot_refresher(snap_clone));

    let listener = TcpListener::bind(format!("0.0.0.0:{}", PORT))
        .expect("Impossible d'écouter sur le port");

    println!("En attente sur le port {}...", PORT);

    for stream in listener.incoming() {
        match stream {
            Ok(s) => {
                let snap = Arc::clone(&shared);
                thread::spawn(move || handle_client(s, snap));
            }
            Err(e) => eprintln!("Erreur connexion: {}", e),
        }
    }
}

// ─── Mode MASTER ──────────────────────────────────────────────────────────────
//
// Permet à une machine de piloter plusieurs agents distants.
// Lancement : cargo run -- --master 192.168.1.10 192.168.1.11
//
// Commandes disponibles depuis le master :
//   list              — affiche les agents enregistrés
//   <N>:<commande>    — envoie la commande à l'agent N  (ex: 1:cpu  2:mem)
//   <N>:all           — vue complète de l'agent N
//   quit              — quitter le mode master

fn send_to_agent(addr: &str, command: &str) -> Result<String, String> {
    let mut stream = TcpStream::connect(addr)
        .map_err(|e| format!("Connexion impossible à {} : {}", addr, e))?;

    let mut reader = BufReader::new(stream.try_clone().unwrap());

    // Lire le prompt "TOKEN: "
    let mut prompt = String::new();
    reader.read_line(&mut prompt).map_err(|e| e.to_string())?;

    // Envoyer le token
    stream.write_all(format!("{}\n", AUTH_TOKEN).as_bytes())
        .map_err(|e| e.to_string())?;

    // Lire la réponse OK / UNAUTHORIZED
    let mut status = String::new();
    reader.read_line(&mut status).map_err(|e| e.to_string())?;
    if status.trim() != "OK" {
        return Err("Token refusé par l'agent".to_string());
    }

    // Envoyer la commande
    stream.write_all(format!("{}\n", command).as_bytes())
        .map_err(|e| e.to_string())?;

    // Lire la réponse jusqu'au marqueur END
    let mut response = String::new();
    for line in reader.lines() {
        match line {
            Ok(l) if l.trim() == "END" => break,
            Ok(l) => {
                response.push_str(&l);
                response.push('\n');
            }
            Err(_) => break,
        }
    }

    // Fermer proprement
    let _ = stream.write_all(b"quit\n");

    Ok(response)
}

fn run_master(agents: Vec<String>) {
    println!("SysWatch — mode MASTER");
    println!("Agents enregistrés :");
    for (i, addr) in agents.iter().enumerate() {
        println!("  [{}] {}", i + 1, addr);
    }
    println!();
    println!("Utilisation :");
    println!("  list          — voir les agents");
    println!("  <N>:<cmd>     — ex: 1:cpu  2:mem  3:all");
    println!("  quit          — quitter\n");

    let stdin_handle = stdin();
    for line in stdin_handle.lock().lines() {
        let input = match line {
            Ok(l) => l.trim().to_string(),
            Err(_) => break,
        };

        if input.is_empty() {
            continue;
        }

        if input == "quit" {
            println!("Au revoir.");
            break;
        }

        if input == "list" {
            for (i, addr) in agents.iter().enumerate() {
                println!("  [{}] {}", i + 1, addr);
            }
            continue;
        }

        // Format attendu : "1:cpu" ou "2:mem"
        if let Some(colon_pos) = input.find(':') {
            let index_str = &input[..colon_pos];
            let cmd       = &input[colon_pos + 1..];

            match index_str.parse::<usize>() {
                Ok(n) if n >= 1 && n <= agents.len() => {
                    let addr = &agents[n - 1];
                    print!("\n[Agent {} — {}]\n", n, addr);
                    match send_to_agent(addr, cmd) {
                        Ok(resp) => print!("{}", resp),
                        Err(e)   => println!("Erreur : {}", e),
                    }
                    println!();
                }
                _ => println!("Index invalide. Agents disponibles : 1 à {}", agents.len()),
            }
        } else {
            println!("Format : <numéro>:<commande>  ex: 1:cpu");
        }
    }
}

// ─── Main ─────────────────────────────────────────────────────────────────────

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() > 1 && args[1] == "--master" {
        // Les arguments suivants sont les adresses des agents
        let agents: Vec<String> = args[2..]
            .iter()
            .map(|a| {
                if a.contains(':') {
                    a.clone()
                } else {
                    format!("{}:{}", a, PORT)
                }
            })
            .collect();

        if agents.is_empty() {
            eprintln!("Usage : cargo run -- --master <ip1> [ip2] [ip3]");
            eprintln!("Exemple : cargo run -- --master 192.168.1.10 192.168.1.11");
            return;
        }

        run_master(agents);
    } else {
        run_agent();
    }
}