#[cfg(test)]
mod account_tests;
mod application;
mod bootstrap;
mod domain;
mod http;
mod infrastructure;
#[cfg(test)]
mod integration_tests;
#[cfg(test)]
mod team_tests;

use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let mut path = std::path::PathBuf::from("rovar-server.toml");
    let mut create = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--config" => {
                path = args
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("--config requires a TOML file"))?
                    .into()
            }
            "--create-user" => {
                create = Some(
                    args.next()
                        .ok_or_else(|| anyhow::anyhow!("--create-user requires a username"))?,
                )
            }
            _ => anyhow::bail!("Usage: rovar-server [--config FILE] [--create-user USERNAME]"),
        }
    }
    let config = bootstrap::config::Config::load(&path)?;
    let app = bootstrap::build(&config).await?;
    if let Some(username) = create {
        // Read a provisioned password from stdin; never store it in configuration.
        use std::io::BufRead;
        let mut password = String::new();
        std::io::stdin().lock().read_line(&mut password)?;
        app.auth
            .create_user(&username, password.trim_end_matches(['\r', '\n']))
            .await?;
        println!("Account created: {username}");
    } else {
        http::serve(config, app).await;
    }
    Ok(())
}
