use anyhow::Result;
use sweeper::cli::Cli;

fn main() -> Result<()> {
    let cli = Cli::parse();
    cli.run()
}
