use clap::{Parser, Subcommand};
use dirs;
use std::fs;
use std::io;
use toml_edit::{value, DocumentMut, Item, Table};

#[derive(Parser, Debug)]
#[command(
    name = "cargo-proxy",
    author,
    version,
    about = "🛠️ Quickly set, view, and clear Cargo proxies to speed up dependency downloads."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Set a proxy configuration, by name or custom URL
    Set {
        /// Proxy name or a custom URL starting with http:// or https://
        proxy: String,
    },
    /// Clear any existing proxy configuration
    Clear,
    /// Show current proxy configuration
    Show,
}

fn predefined_proxies() -> Vec<(&'static str, &'static str)> {
    vec![
        ("rsproxy", "https://rsproxy.cn/crates.io-index/"),
        ("ustc", "https://mirrors.ustc.edu.cn/crates.io-index/"),
        (
            "tuna",
            "https://mirrors.tuna.tsinghua.edu.cn/crates.io-index/",
        ),
        ("aliyun", "https://mirrors.aliyun.com/crates.io-index/"),
    ]
}

fn set_proxy(proxy: &str) -> io::Result<()> {
    let (proxy_name, proxy_url) = if let Some((name, url)) = predefined_proxies()
        .iter()
        .find(|(name, _)| *name == proxy.to_lowercase())
    {
        (name.to_string(), url.to_string())
    } else if proxy.starts_with("http://") || proxy.starts_with("https://") {
        ("custom".to_string(), proxy.to_string())
    } else {
        let mut err_msg = String::from("Error: Unknown proxy name or invalid URL.\n");
        err_msg.push_str("Available predefined proxy names:\n");
        for (name, _) in predefined_proxies() {
            err_msg.push_str(&format!("  - {}\n", name));
        }
        err_msg.push_str("Or provide a custom URL starting with http:// or https://.\n");
        return Err(io::Error::new(io::ErrorKind::InvalidInput, err_msg));
    };

    let config_path = dirs::home_dir().unwrap().join(".cargo").join("config.toml");

    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut doc = if config_path.exists() {
        let content = fs::read_to_string(&config_path)?;
        content
            .parse::<DocumentMut>()
            .unwrap_or_else(|_| DocumentMut::new())
    } else {
        DocumentMut::new()
    };

    // Backup existing config file
    if config_path.exists() {
        let backup_path = config_path.with_extension("backup");
        fs::copy(&config_path, &backup_path)?;
        println!(
            "Existing configuration backed up to {}",
            backup_path.to_string_lossy()
        );
    }

    // Remove existing proxy configuration
    remove_proxy_config(&mut doc);

    // Add new proxy configuration
    add_proxy_config(&mut doc, &proxy_name, &proxy_url);

    fs::write(&config_path, doc.to_string())?;

    println!(
        "Proxy configuration set to {}, config file: {}",
        proxy_url,
        config_path.to_string_lossy()
    );

    Ok(())
}

fn clear_proxy() -> io::Result<()> {
    let config_path = dirs::home_dir().unwrap().join(".cargo").join("config.toml");

    if !config_path.exists() {
        println!("No configuration file found. Nothing to clear.");
        return Ok(());
    }

    let backup_path = config_path.with_extension("backup");
    fs::copy(&config_path, &backup_path)?;
    println!(
        "Existing configuration backed up to {}",
        backup_path.to_string_lossy()
    );

    let content = fs::read_to_string(&config_path)?;
    let mut doc = content
        .parse::<DocumentMut>()
        .unwrap_or_else(|_| DocumentMut::new());

    remove_proxy_config(&mut doc);
    fs::write(&config_path, doc.to_string())?;
    println!("Proxy configuration has been successfully cleared.");

    Ok(())
}

fn show_proxy() -> io::Result<()> {
    let config_path = dirs::home_dir().unwrap().join(".cargo").join("config.toml");

    if !config_path.exists() {
        println!("No proxy is currently set.");
        return Ok(());
    }

    let content = fs::read_to_string(&config_path)?;
    let doc = content
        .parse::<DocumentMut>()
        .unwrap_or_else(|_| DocumentMut::new());

    let replace_with = doc
        .get("source")
        .and_then(|s| s.get("crates-io"))
        .and_then(|ci| ci.as_table())
        .and_then(|t| t.get("replace-with"))
        .and_then(|v| v.as_value())
        .and_then(|val| val.as_str());

    if replace_with.is_none() {
        println!("No proxy is currently set.");
        return Ok(());
    }

    let current_replace = replace_with.unwrap();

    if current_replace == "rsproxy-sparse" {
        let registry_url = doc
            .get("source")
            .and_then(|s| s.get("rsproxy"))
            .and_then(|ci| ci.as_table())
            .and_then(|t| t.get("registry"))
            .and_then(|v| v.as_value())
            .and_then(|val| val.as_str());

        if let Some(url) = registry_url {
            if let Some((name, _)) = predefined_proxies().iter().find(|(_, u)| *u == url) {
                println!("Current proxy: {}", name);
            } else {
                println!("Current proxy: {}", url);
            }
        } else {
            println!("No proxy is currently set.");
        }
    } else {
        let registry_url = doc
            .get("source")
            .and_then(|s| s.get(current_replace))
            .and_then(|ci| ci.as_table())
            .and_then(|t| t.get("registry"))
            .and_then(|v| v.as_value())
            .and_then(|val| val.as_str());

        if let Some(url) = registry_url {
            if let Some((name, _)) = predefined_proxies()
                .iter()
                .find(|(n, u)| *n == current_replace || *u == url)
            {
                println!("Current proxy: {}", name);
            } else {
                println!("Current proxy: {}", url);
            }
        } else {
            println!("No proxy is currently set.");
        }
    }

    Ok(())
}

fn remove_proxy_config(doc: &mut DocumentMut) {
    if let Some(source_table) = doc.as_table_mut().get_mut("source") {
        if let Item::Table(t) = source_table {
            let keys: Vec<String> = t.iter().map(|(k, _)| k.to_string()).collect();
            for k in keys {
                if k != "crates-io" {
                    t.remove(&k);
                } else {
                    if let Some(Item::Table(ci)) = t.get_mut("crates-io") {
                        ci.remove("replace-with");
                    }
                }
            }
        }
    }

    if let Some(registries_table) = doc.as_table_mut().get_mut("registries") {
        if let Item::Table(registries) = registries_table {
            let keys: Vec<String> = registries.iter().map(|(k, _)| k.to_string()).collect();
            for k in keys {
                registries.remove(&k);
            }
        }
    }

    if let Some(net_table) = doc.as_table_mut().get_mut("net") {
        if let Item::Table(net) = net_table {
            net.remove("git-fetch-with-cli");
        }
    }
}

fn add_proxy_config(doc: &mut DocumentMut, proxy_name: &str, proxy_url: &str) {
    let source_table = doc
        .as_table_mut()
        .entry("source")
        .or_insert(Item::Table(Table::new()));
    let crates_io_table = ensure_table(source_table, "crates-io");

    if proxy_name == "rsproxy" {
        crates_io_table.insert("replace-with", value("rsproxy-sparse"));
        let rsproxy_table = ensure_table(source_table, "rsproxy");
        rsproxy_table.insert("registry", value("https://rsproxy.cn/crates.io-index"));

        let rsproxy_sparse_table = ensure_table(source_table, "rsproxy-sparse");
        rsproxy_sparse_table.insert("registry", value("sparse+https://rsproxy.cn/index/"));

        let registries_table = doc
            .as_table_mut()
            .entry("registries")
            .or_insert(Item::Table(Table::new()));
        let rsproxy_registry_table = ensure_table(registries_table, "rsproxy");
        rsproxy_registry_table.insert("index", value("https://rsproxy.cn/crates.io-index"));

        let net_table = doc
            .as_table_mut()
            .entry("net")
            .or_insert(Item::Table(Table::new()));
        if let Item::Table(nt) = net_table {
            nt.insert("git-fetch-with-cli", value(true));
        }
    } else {
        crates_io_table.insert("replace-with", value(proxy_name));

        let trimmed_url = proxy_url.trim_end_matches('/');
        let sparse_url = format!("sparse+{}/", trimmed_url);

        let proxy_source_table = ensure_table(source_table, proxy_name);
        proxy_source_table.insert("registry", value(sparse_url));
    }
}

fn ensure_table<'a>(item: &'a mut Item, key: &str) -> &'a mut Table {
    match item {
        Item::Table(t) => {
            let entry = t.entry(key).or_insert(Item::Table(Table::new()));
            if let Item::Table(tbl) = entry {
                tbl
            } else {
                let new_table = Table::new();
                *entry = Item::Table(new_table);
                if let Item::Table(tbl) = entry {
                    tbl
                } else {
                    panic!("Failed to ensure table");
                }
            }
        }
        _ => panic!("ensure_table called on non-table item"),
    }
}

fn main() -> io::Result<()> {
    let args = Cli::parse();

    match args.command {
        Command::Set { proxy } => {
            if let Err(e) = set_proxy(&proxy) {
                eprintln!("{}", e);
                std::process::exit(1);
            }
        }
        Command::Clear => {
            if let Err(e) = clear_proxy() {
                eprintln!("{}", e);
                std::process::exit(1);
            }
        }
        Command::Show => {
            if let Err(e) = show_proxy() {
                eprintln!("{}", e);
                std::process::exit(1);
            }
        }
    }

    Ok(())
}
