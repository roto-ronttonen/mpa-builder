use std::{
    collections::HashMap,
    fs::{self, File},
    hash::Hash,
    path::{Path, PathBuf},
    process::{self, Child, Command},
    sync::{
        mpsc::{Receiver, Sender},
        Arc, Mutex,
    },
    thread,
};

use clap::{Parser, Subcommand};
use dialoguer::{theme::ColorfulTheme, Input};
use glob::glob;
use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use refresh_server::start_refresh_server;
use rust_embed::RustEmbed;
use serde::Serialize;
use serde_json::{Map, Value};

mod refresh_server;

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

fn src_path_to_dist_path(p: &str) -> String {
    p.replace("src/", "dist/")
}

fn copy_dir_all(src: impl AsRef<Path>, dst: impl AsRef<Path>) {
    fs::create_dir_all(&dst).unwrap();
    for entry in fs::read_dir(src).unwrap() {
        let entry = entry.unwrap();
        let ty = entry.file_type().unwrap();
        if ty.is_dir() {
            copy_dir_all(entry.path(), dst.as_ref().join(entry.file_name()));
        } else {
            fs::copy(entry.path(), dst.as_ref().join(entry.file_name())).unwrap();
        }
    }
}

fn build(dev: bool) {
    let dist_path = Path::new("dist");
    if dist_path.exists() {
        fs::remove_dir_all(dist_path).unwrap();
    }
    fs::create_dir(dist_path).unwrap();
    fs::create_dir_all(dist_path.join("styles")).unwrap();
    fs::create_dir_all(dist_path.join("scripts")).unwrap();
    fs::create_dir_all(dist_path.join("media")).unwrap();
    println!("Generating tailwind");
    run_command_and_wait(
        "npx",
        Some(vec![
            "tailwindcss",
            "-i",
            "./src/styles/tailwind.css",
            "-o",
            "./dist/styles/tailwind.css",
        ]),
        None,
    );
    let scripts_p = Path::new("src/scripts");
    if scripts_p.exists() {
        println!("Generating js");
        for entry in glob("src/scripts/**/*.js").unwrap() {
            match entry {
                Ok(path) => {
                    let path_str = path.to_str().unwrap();
                    run_command_and_wait(
                        "npx",
                        Some(vec![
                            "esbuild",
                            &path_str,
                            &format!("--outfile={}", &src_path_to_dist_path(path_str)),
                            "--bundle",
                            "--minify",
                            "--target=chrome58,firefox57,safari11,edge16",
                            "--external:../node_modules/*",
                        ]),
                        None,
                    )
                }
                Err(_) => panic!("failed to read script"),
            }
        }
    }

    println!("Generating css");
    for entry in glob("src/styles/**/*.css").unwrap() {
        match entry {
            Ok(path) => {
                let path_str = path.to_str().unwrap();
                if !path_str.ends_with("tailwind.css") {
                    fs::copy(path_str, src_path_to_dist_path(path_str)).unwrap();
                }
            }
            Err(_) => panic!("failed to read style"),
        }
    }

    let mut intl_map: HashMap<String, HashMap<String, HashMap<String, String>>> = HashMap::new();
    // Always have default intl incase translations are not used
    intl_map.insert("default".to_string(), HashMap::new());
    let intl_p = Path::new("src/intl");
    if intl_p.exists() {
        println!("Generating translations");
        for entry in glob("src/intl/**/*.json").unwrap() {
            match entry {
                Ok(path) => {
                    let path_str = path.to_str().unwrap();
                    let mut map = HashMap::new();
                    let content = fs::read_to_string(&path).unwrap();
                    map = serde_json::from_str(&content).unwrap();
                    if path_str.ends_with("_default.json") {
                        intl_map.insert("default".to_string(), map.clone());
                    }
                    let normalized_path = path_str.replace("_default.json", ".json");
                    let splitted_path = normalized_path.split("/").collect::<Vec<&str>>();

                    let lang = splitted_path.last().unwrap().replace(".json", "");
                    intl_map.insert(lang.to_string(), map.clone());
                }
                Err(_) => panic!("failed to read intl"),
            }
        }
    }

    println!("Generating html");
    let layout_html = fs::read_to_string("src/layout.html").unwrap();
    for entry in glob("src/pages/**/*.html").unwrap() {
        match entry {
            Ok(path) => {
                let path_str = path.to_str().unwrap();
                let splitted_path = path_str.split("/").collect::<Vec<&str>>();
                let page_name = splitted_path.last().unwrap().replace(".html", "");
                let page_content = fs::read_to_string(path_str).unwrap();
                for (key, value) in intl_map.clone().into_iter() {
                    /* Magic keys are: title */
                    #[derive(Serialize)]
                    struct LayoutData {
                        title: String,
                        content: String,
                    }
                    let title_map = value.get("title").unwrap_or(&HashMap::new()).to_owned();

                    let layout_data = LayoutData {
                        title: title_map
                            .get(&page_name.to_owned())
                            .unwrap_or(&"".to_string())
                            .to_owned(),
                        content: page_content.clone(),
                    };
                    let layout_template = mustache::compile_str(&layout_html).unwrap();
                    let mut layout_bytes = vec![];
                    layout_template
                        .render(&mut layout_bytes, &layout_data)
                        .unwrap();
                    let mut layout_rendered =
                        std::str::from_utf8(&layout_bytes).unwrap().to_string();
                    if dev {
                        let mut splitted = layout_rendered.split("</body>").collect::<Vec<&str>>();
                        let mut st = splitted[0].to_owned();
                        st += "<script>
                            let token = sessionStorage.getItem('refresherToken');
                            const refresher = async () => {
                                try {
                                    const res = await fetch('localhost:4242')
                                    const t = await res.text()
                                    if (t !== token) {
                                        sessionStorage.setItem('refresherToken', t);
                                        window.location.reload();
                                    }
                                } finally {
                                    setTimeout(refresher,500);
                                }
                            }
                            refresher()
                        </script>";
                        splitted[0] = &st;
                        layout_rendered = splitted.join("</body>");
                    }
                    let page_template = mustache::compile_str(&layout_rendered).unwrap();
                    let mut page_bytes = vec![];
                    let page_data = value.get(&page_name).unwrap_or(&HashMap::new()).to_owned();
                    page_template.render(&mut page_bytes, &page_data).unwrap();
                    let mut path = dist_path.to_owned();
                    if key != "default" {
                        path = path.join(key);
                        fs::create_dir_all(&path).unwrap();
                    }
                    path = path.join(format!("{page_name}.html"));

                    fs::write(path, std::str::from_utf8(&page_bytes).unwrap()).unwrap();
                }
            }
            Err(_) => panic!("failed to read page"),
        }
    }

    let media_p = Path::new("src/media");
    if media_p.exists() {
        println!("generating media");
        copy_dir_all("src/media", "dist/media");
    }
}

// static mut processing: bool = false;

fn watch(refresher_token: Arc<Mutex<i32>>) {
    let (tx, rx) = std::sync::mpsc::channel();

    // Automatically select the best implementation for your platform.
    // You can also access each implementation directly e.g. INotifyWatcher.
    let mut watcher = RecommendedWatcher::new(tx, Config::default()).unwrap();

    // Add a path to be watched. All files and directories at that path and
    // below will be monitored for changes.
    watcher
        .watch("src".as_ref(), RecursiveMode::Recursive)
        .unwrap();

    build(true);
    start_dev_server();
    let processing = Arc::new(Mutex::new(false));
    for res in rx {
        match res {
            Ok(event) => unsafe {
                let processing_handle = processing.clone();
                let mut curr_processing = false;
                {
                    curr_processing = *processing_handle.lock().unwrap();
                }
                if !curr_processing {
                    let processing_handle_thread = processing.clone();
                    {
                        *processing_handle.lock().unwrap() = true;
                    }
                    println!("File changed, restarting");
                    let r_token = refresher_token.clone();
                    thread::spawn(move || {
                        build(true);
                        refresh_refresher_token(r_token);
                        *processing_handle_thread.lock().unwrap() = false;
                    });
                }
            },
            Err(e) => panic!("watch error: {:?}", e),
        }
    }
}

fn start_dev_server() -> Child {
    Command::new("npx")
        .args(vec!["serve", "dist"])
        .spawn()
        .unwrap()
}

fn refresh_refresher_token(token: Arc<Mutex<i32>>) {
    let mut t = token.lock().unwrap();
    *t += 1;
}

fn main() {
    let refresher_token = Arc::new(Mutex::new(0));
    let cli = Cli::parse();
    match &cli.command {
        Commands::Dev => {
            watch(refresher_token.clone());
            start_refresh_server(refresher_token.clone());
        }
        Commands::Build => {
            build(false);
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
                "favicon.ico",
                "robots.txt",
                "layout.html",
                "styles/tailwind.css",
                "scripts/base.js",
                "scripts/about.js",
                "scripts/home.js",
                "pages/index.html",
                "pages/about.html",
                "media/sample.png",
                "intl/en.json",
                "intl/fi_default.json",
            ];
            for f in files {
                let splitted = f.split("/");
                let vec = splitted.collect::<Vec<&str>>();
                if vec.len() == 1 {
                    fs::write(src_p.join(vec[0]), Asset::get(f).unwrap().data).unwrap();
                } else {
                    let dir = vec.split_last().unwrap().1;
                    let dir_p = src_p.join(dir.join("/"));
                    fs::create_dir_all(dir_p).unwrap();
                    fs::write(src_p.join(vec.join("/")), Asset::get(f).unwrap().data).unwrap();
                }
            }

            run_command_and_wait("npm", Some(vec!["init", "-y"]), Some(&input));
            run_command_and_wait(
                "npm",
                Some(vec![
                    "install",
                    "--save-dev",
                    "tailwindcss",
                    "esbuild",
                    "serve",
                ]),
                Some(&input),
            );
            run_command_and_wait("npm", Some(vec!["install", "lodash"]), Some(&input));
            run_command_and_wait("npx", Some(vec!["tailwindcss", "init"]), Some(&input));
            fs::write(
                Path::new(&input).join("tailwind.config.js"),
                asset_to_string("tailwind.config.js"),
            )
            .unwrap();
            let node_version_file = File::create(Path::new(&input).join(".node-version")).unwrap();
            let mut node_version_cmd = Command::new("node")
                .args(vec!["--version"])
                .stdout(node_version_file)
                .spawn()
                .unwrap();
            node_version_cmd.wait().unwrap();
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
