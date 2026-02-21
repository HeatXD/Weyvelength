use clap::Parser;

mod codegen;
mod service;
mod session;
mod state;

use state::IceServerConfig;

#[derive(Parser)]
#[command(about = "Weyvelength gRPC server")]
struct Cli {
    #[arg(long, default_value = "0.0.0.0")]
    host: String,
    #[arg(long, default_value_t = 50051)]
    port: u16,
    #[arg(long, default_value = "Weyvelength Server")]
    name: String,
    #[arg(long, default_value = "Welcome!")]
    motd: String,
    /// STUN server URL (repeatable). Defaults to Google's public STUN server.
    /// Example: --stun stun:stun.l.google.com:19302
    #[arg(long = "stun", default_value = "stun:stun.l.google.com:19302")]
    stun: Vec<String>,
    /// Named TURN server (repeatable). Format: "Display Name|turn:host:port|username|credential"
    /// Example: --turn "US East|turn:us.example.com:3478|alice|s3cr3t"
    #[arg(long = "turn", value_name = "NAME|URL|USER|CRED")]
    turn: Vec<String>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let addr = format!("{}:{}", cli.host, cli.port).parse()?;

    let mut ice_servers: Vec<IceServerConfig> = cli
        .stun
        .into_iter()
        .map(|url| IceServerConfig { url, username: String::new(), credential: String::new(), name: String::new() })
        .collect();

    for spec in cli.turn {
        let parts: Vec<&str> = spec.splitn(4, '|').collect();
        if parts.len() == 4 {
            ice_servers.push(IceServerConfig {
                name:       parts[0].to_string(),
                url:        parts[1].to_string(),
                username:   parts[2].to_string(),
                credential: parts[3].to_string(),
            });
        } else {
            eprintln!("Warning: ignoring malformed --turn value (expected Name|url|user|cred): {spec}");
        }
    }

    service::run(addr, cli.name, cli.motd, ice_servers).await
}
