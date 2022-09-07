use std::{
    fs,
    path::Path,
    process::{self, Command},
};

use clap::{Parser, Subcommand};
use dialoguer::{theme::ColorfulTheme, Input};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "init_template/"]
struct Asset;

#[derive(Parser)]
#[clap(author, version, about, long_about = None)]
#[clap(propagate_version = true)]
struct Cli {
    #[clap(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Dev,
    Build,
    New,
}

fn main() {
    let cli = Cli::parse();
    match &cli.command {
        Commands::Dev => {
            println!("TODO")
        }
        Commands::Build => {
            println!("TODO")
        }
        Commands::New => {
            let input: String = Input::with_theme(&ColorfulTheme::default())
                .with_prompt("Output directory")
                .interact_text()
                .unwrap();
            if Path::new(&input).exists() {
                println!("{input} already exists");
                process::exit(1);
            }

            let src_p = Path::new(&input).join("src");
            fs::create_dir_all(&src_p).unwrap();
            let files = vec![
                "layout.html",
                "styles/tailwind.css",
                "scripts/base.ts",
                "pages/index.html",
                "pages/about.html",
                "intl/en.json",
                "intl/fi_default.json",
            ];
            for f in files {
                let splitted = f.split("/");
                let vec = splitted.collect::<Vec<&str>>();
                if vec.len() == 1 {
                    fs::write(src_p.join(vec[0]), asset_to_string(f)).unwrap();
                } else {
                    let dir = vec.split_last().unwrap().1;
                    let dir_p = src_p.join(dir.join("/"));
                    fs::create_dir_all(dir_p).unwrap();
                    fs::write(src_p.join(vec.join("/")), asset_to_string(f)).unwrap();
                }
            }

            run_command_and_wait("npm", Some(vec!["init", "-y"]), Some(&input));
            run_command_and_wait(
                "npm",
                Some(vec!["install", "--save-dev", "typescript", "tailwindcss"]),
                Some(&input),
            );
        }
    }
}

fn asset_to_string(path: &str) -> String {
    let src = Asset::get(path).unwrap();
    let s = std::str::from_utf8(src.data.as_ref()).unwrap();
    String::from(s)
}

fn run_command_and_wait(prog: &str, args: Option<Vec<&str>>, directory: Option<&String>) {
    let mut cmd = Command::new(prog);
    if args.is_some() {
        cmd.args(args.unwrap());
    }
    if directory.is_some() {
        cmd.current_dir(directory.unwrap());
    }

    let child = cmd.spawn().unwrap();
    child.wait_with_output().unwrap();
}
