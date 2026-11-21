//! Print the mailbox totals Gmail reports for a signed-in account, with the
//! stored refresh token and `config.toml`. Debug aid for sync progress.
//!
//!     cargo run -p tidemail-gmail --example profile -- <email>

use std::sync::Arc;

use tidemail_gmail::{load_config, Endpoints, GmailClient, KeyringStore, TokenSource};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let email = std::env::args().nth(1).ok_or("usage: profile <email>")?;
    let tokens = TokenSource::new(
        load_config()?,
        Endpoints::default().token_url,
        email,
        Arc::new(KeyringStore),
    );
    let client = GmailClient::new(Arc::new(tokens));
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build()?;
    let p = rt.block_on(client.profile())?;
    println!(
        "messagesTotal {}  threadsTotal {}",
        p.messages_total.unwrap_or(-1),
        p.threads_total.unwrap_or(-1)
    );
    Ok(())
}
