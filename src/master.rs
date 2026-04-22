// src/master.rs
// Interface maître SysWatch — tourne sur le PC du professeur

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::time::Duration;

const AUTH_TOKEN: &str = "ENSPD2026";
const PORT: u16 = 7878;

// Liste statique des machines — à remplir avec les IPs des PC étudiants
// En cours : chaque étudiant communique son IP via `ipconfig`
fn machines() -> HashMap<String, String> {
    let mut m = HashMap::new();
    // format : "nom_affichage" => "ip"
    m.insert("PC-01-TIEMOUO".to_string(), "192.168.1.101".to_string());
    m.insert("PC-02-FOUEJIO".to_string(), "192.168.1.102".to_string());
    m.insert("PC-03-NGOOH".to_string(), "192.168.1.103".to_string());
    m.insert("MAIMOUNA".to_string(), "192.168.1.105".to_string());
    // Ajouter autant de lignes que d'étudiants
    m
}

struct AgentSession {
    #[allow(dead_code)]
    name: String,
    #[allow(dead_code)]
    ip: String,
    stream: TcpStream,
    reader: BufReader<TcpStream>,
}

impl AgentSession {
    fn connect(name: &str, ip: &str) -> Result<Self, String> {
        let addr = format!("{}:{}", ip, PORT);
        let stream = TcpStream::connect_timeout(
            &addr.parse().map_err(|e| format!("{}", e))?,
            Duration::from_secs(2),
        )
        .map_err(|e| format!("Connexion refusée: {}", e))?;

        stream.set_read_timeout(Some(Duration::from_secs(5))).ok();

        let mut session = AgentSession {
            name: name.to_string(),
            ip: ip.to_string(),
            stream: stream.try_clone().unwrap(),
            reader: BufReader::new(stream),
        };

        // Authentification
        session.read_until_prompt("TOKEN: ")?;
        session.send(AUTH_TOKEN)?;
        let resp = session.read_line()?;
        if resp.trim() != "OK" {
            return Err("Token refusé".to_string());
        }

        Ok(session)
    }

    fn send(&mut self, cmd: &str) -> Result<(), String> {
        self.stream
            .write_all(format!("{}\n", cmd).as_bytes())
            .map_err(|e| e.to_string())
    }

    fn read_line(&mut self) -> Result<String, String> {
        let mut line = String::new();
        self.reader
            .read_line(&mut line)
            .map_err(|e| e.to_string())?;
        Ok(line)
    }

    fn read_until_end(&mut self) -> Result<String, String> {
        let mut result = String::new();
        loop {
            let mut line = String::new();
            match self.reader.read_line(&mut line) {
                Ok(0) => break,
                Ok(_) => {
                    if line.trim() == "END" {
                        break;
                    }
                    result.push_str(&line);
                }
                Err(_) => break,
            }
        }
        Ok(result)
    }

    fn read_until_prompt(&mut self, prompt: &str) -> Result<(), String> {
        let mut buf = String::new();
        loop {
            let mut line = String::new();
            self.reader.read_line(&mut line).map_err(|e| e.to_string())?;
            buf.push_str(&line);
            if buf.contains(prompt) {
                return Ok(());
            }
        }
    }

    fn run_command(&mut self, cmd: &str) -> String {
        match self.send(cmd) {
            Err(e) => format!("Erreur envoi: {}", e),
            Ok(_) => self.read_until_end().unwrap_or_else(|e| format!("Erreur lecture: {}", e)),
        }
    }
}

// Scan du réseau : tenter de joindre toutes les machines configurées
fn scan_machines() -> Vec<(String, String, bool)> {
    let machines = machines();
    let mut results = vec![];

    println!("Scan du réseau...");
    for (name, ip) in &machines {
        let addr = format!("{}:{}", ip, PORT);
        let reachable = TcpStream::connect_timeout(
            &addr.parse().unwrap(),
            Duration::from_secs(1),
        )
        .is_ok();
        let status = if reachable { "✓ EN LIGNE" } else { "✗ HORS LIGNE" };
        println!("  {} ({}) — {}", name, ip, status);
        results.push((name.clone(), ip.clone(), reachable));
    }
    results
}

fn connect_to(name: &str, ip: &str) -> Option<AgentSession> {
    match AgentSession::connect(name, ip) {
        Ok(s) => {
            println!("  [✓] Connecté à {} ({})", name, ip);
            Some(s)
        }
        Err(e) => {
            println!("  [✗] {} ({}) — {}", name, ip, e);
            None
        }
    }
}

fn print_menu() {
    println!("\n╔══════════════════════════════════════════════╗");
    println!("║        SYSWATCH MASTER — ENSPD 2026         ║");
    println!("╠══════════════════════════════════════════════╣");
    println!("║  scan          — lister les machines         ║");
    println!("║  select <nom>  — cibler une machine          ║");
    println!("║  all <cmd>     — envoyer cmd à toutes        ║");
    println!("╠══════════════════════════════════════════════╣");
    println!("║  Commandes disponibles sur les agents :      ║");
    println!("║  cpu / mem / ps / all                        ║");
    println!("║  msg <texte>   — afficher message            ║");
    println!("║  install <pkg> — installer un logiciel       ║");
    println!("║  shutdown      — éteindre la machine         ║");
    println!("║  reboot        — redémarrer                  ║");
    println!("║  abort         — annuler extinction          ║");
    println!("╠══════════════════════════════════════════════╣");
    println!("║  help          — afficher ce menu            ║");
    println!("║  quit          — quitter le master           ║");
    println!("╚══════════════════════════════════════════════╝");
}

fn main() {
    print_menu();

    let machines_list = machines();
    let mut selected_name: Option<String> = None;
    let stdin = std::io::stdin();

    loop {
        // Prompt
        let prompt = match &selected_name {
            Some(name) => format!("[master@{}]> ", name),
            None => "[master]> ".to_string(),
        };
        print!("{}", prompt);
        std::io::stdout().flush().unwrap();

        let mut input = String::new();
        stdin.lock().read_line(&mut input).unwrap();
        let input = input.trim().to_string();

        if input.is_empty() {
            continue;
        }

        match input.as_str() {
            "quit" | "exit" => {
                println!("Au revoir.");
                break;
            }

            "help" => print_menu(),

            "scan" => {
                scan_machines();
            }

            _ if input.starts_with("select ") => {
                let name = input[7..].trim().to_string();
                if machines_list.contains_key(&name) {
                    selected_name = Some(name.clone());
                    println!("Machine sélectionnée : {}", name);
                } else {
                    println!("Machine inconnue : '{}'. Lance 'scan' pour voir les machines.", name);
                }
            }

            _ if input.starts_with("all ") => {
                // Envoyer la commande à TOUTES les machines en ligne
                let cmd = input[4..].trim().to_string();
                println!("Envoi de '{}' à toutes les machines...", cmd);

                for (name, ip) in &machines_list {
                    print!("  {} — ", name);
                    std::io::stdout().flush().unwrap();
                    match connect_to(name, ip) {
                        Some(mut session) => {
                            let response = session.run_command(&cmd);
                            // Afficher juste la première ligne pour ne pas noyer la console
                            let first_line = response.lines().next().unwrap_or("(vide)");
                            println!("{}", first_line);
                        }
                        None => println!("hors ligne"),
                    }
                }
            }

            // Commande vers la machine sélectionnée
            cmd => {
                match &selected_name.clone() {
                    None => println!("Aucune machine sélectionnée. Utilise 'select <nom>' ou 'all <cmd>'."),
                    Some(name) => {
                        let ip = machines_list[name].clone();
                        match connect_to(name, &ip) {
                            None => println!("Machine hors ligne."),
                            Some(mut session) => {
                                let response = session.run_command(cmd);
                                println!("{}", response);
                            }
                        }
                    }
                }
            }
        }
    }
}