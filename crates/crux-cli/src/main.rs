use clap::Parser;

#[derive(Parser)]
#[command(name = "crux", version, about = "CLI output compressor for AI agents")]
struct Cli {
    /// Command to run and compress
    #[arg(trailing_var_arg = true)]
    command: Vec<String>,
}

fn main() {
    let cli = Cli::parse();

    if cli.command.is_empty() {
        eprintln!("Usage: crux <command> [args...]");
        std::process::exit(1);
    }

    // TODO: resolve filter, execute command, apply filter, output
    println!("crux: would run {:?}", cli.command);
}
