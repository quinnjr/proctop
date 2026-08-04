use std::process::ExitCode;

use clap::Parser;
use ntui::{element, render};
use rtop::config::{self, Config, MIN_REFRESH_MS};
use rtop::sort::SortKey;
use rtop::ui::app::{App, AppProps};
use rtop::ui::palette::Palette;

/// An htop-inspired system monitor.
///
/// Flags override the config file for this run only; nothing is written
/// back.
#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    /// Milliseconds between samples.
    #[arg(short, long, value_name = "MS")]
    refresh: Option<u64>,

    /// Column to sort by: pid, name, cpu, mem, time.
    #[arg(short, long)]
    sort: Option<String>,

    /// Colour theme.
    #[arg(short, long)]
    theme: Option<String>,

    /// Start in tree view.
    #[arg(long)]
    tree: bool,

    /// Hide kernel threads.
    #[arg(short = 'H', long)]
    hide_kernel_threads: bool,

    /// Read this config file instead of the default location.
    #[arg(short, long, value_name = "PATH")]
    config: Option<String>,

    /// Print the config file path rtop would read, then exit.
    #[arg(long)]
    show_config_path: bool,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    let cli = Cli::parse();

    if cli.show_config_path {
        println!("{}", config::default_path().display());
        return ExitCode::SUCCESS;
    }

    // A broken config is fatal and reported, rather than silently ignored:
    // a setting that appears not to work with nothing saying why is worse
    // than a refusal to start.
    let config = match load_config(&cli) {
        Ok(config) => config,
        Err(message) => {
            eprintln!("rtop: {message}");
            return ExitCode::FAILURE;
        }
    };

    let palette = match Palette::named(&config.theme) {
        Ok(palette) => palette,
        Err(e) => {
            eprintln!("rtop: {e}");
            return ExitCode::FAILURE;
        }
    };

    match render(element!(App(config: config, palette: palette))).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("rtop: {e}");
            ExitCode::FAILURE
        }
    }
}

fn load_config(cli: &Cli) -> Result<Config, String> {
    // An explicitly named config file must exist; the default one need not.
    let mut config = match &cli.config {
        Some(path) => Config::load_from(path).map_err(|e| e.to_string())?,
        None => Config::load().map_err(|e| e.to_string())?,
    };

    if let Some(refresh) = cli.refresh {
        if refresh < MIN_REFRESH_MS {
            return Err(format!("--refresh must be at least {MIN_REFRESH_MS}ms"));
        }
        config.refresh_ms = refresh;
    }
    if let Some(sort) = &cli.sort {
        config.processes.sort_by = parse_sort(sort)?;
    }
    if let Some(theme) = &cli.theme {
        config.theme = theme.clone();
    }
    if cli.tree {
        config.tree_view = true;
    }
    if cli.hide_kernel_threads {
        config.hide_kernel_threads = true;
    }

    Ok(config)
}

fn parse_sort(word: &str) -> Result<SortKey, String> {
    Ok(match word {
        "pid" => SortKey::Pid,
        "name" | "command" => SortKey::Name,
        "cpu" => SortKey::Cpu,
        "mem" | "memory" | "res" => SortKey::Memory,
        "time" => SortKey::Time,
        other => {
            return Err(format!(
                "--sort expects one of pid, name, cpu, mem, time (got {other})"
            ));
        }
    })
}
