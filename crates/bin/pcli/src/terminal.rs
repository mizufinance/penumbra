use std::io::{IsTerminal, Read, Write};

use anyhow::Result;
use async_trait::async_trait;
use shieldd_sdk_custody::threshold::{SigningRequest, Terminal};
use shieldd_sdk_keys::FullViewingKey;
use shieldd_sdk_transaction::TransactionPlan;
use termion::{color, input::TermRead};

async fn read_password(prompt: &str) -> Result<String> {
    fn get_possibly_empty_string(prompt: &str) -> Result<String> {
        // The `rpassword` crate doesn't support reading from stdin, so we check
        // for an interactive session. We must support non-interactive use cases,
        // for integration with other tooling.
        if std::io::stdin().is_terminal() {
            Ok(rpassword::prompt_password(prompt)?)
        } else {
            Ok(std::io::stdin().lock().read_line()?.unwrap_or_default())
        }
    }

    let mut string: String = Default::default();
    while string.is_empty() {
        // Keep trying until the user provides an input
        string = get_possibly_empty_string(prompt)?;
    }
    Ok(string)
}

fn pretty_print_transaction_plan(
    _fvk: Option<FullViewingKey>,
    plan: &TransactionPlan,
) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(plan)?);
    Ok(())
}

/// For threshold custody, we need to implement this weird terminal abstraction.
///
/// This actually does stuff to stdin and stdout.
#[derive(Clone, Default)]
pub struct ActualTerminal {
    pub fvk: Option<FullViewingKey>,
}

#[async_trait]
impl Terminal for ActualTerminal {
    async fn confirm_request(&self, signing_request: &SigningRequest) -> Result<bool> {
        match signing_request {
            SigningRequest::TransactionPlan(plan) => {
                pretty_print_transaction_plan(self.fvk.clone(), plan)?;
                println!("Do you approve this transaction?");
            }
        };

        println!("Press enter to continue");
        self.read_line_raw().await?;
        Ok(true)
    }

    fn explain(&self, msg: &str) -> Result<()> {
        println!(
            "{}{}{}",
            color::Fg(color::Blue),
            msg,
            color::Fg(color::Reset)
        );
        Ok(())
    }

    async fn broadcast(&self, data: &str) -> Result<()> {
        println!(
            "\n{}{}{}\n",
            color::Fg(color::Yellow),
            data,
            color::Fg(color::Reset)
        );
        Ok(())
    }

    async fn read_line_raw(&self) -> Result<String> {
        // Use raw mode to allow reading more than 1KB/4KB of data at a time
        // See https://unix.stackexchange.com/questions/204815/terminal-does-not-accept-pasted-or-typed-lines-of-more-than-1024-characters
        use termion::raw::IntoRawMode;
        tracing::debug!("about to enter raw mode for long pasted input");

        print!("{}", color::Fg(color::Red));
        // In raw mode, the input is not mirrored into the terminal, so we need
        // to read char-by-char and echo it back.
        let mut stdout = std::io::stdout().into_raw_mode()?;

        let mut bytes = Vec::with_capacity(8192);
        for b in std::io::stdin().bytes() {
            let b = b?;
            // In raw mode, we need to handle control characters ourselves
            if b == 3 || b == 4 {
                // Ctrl-C or Ctrl-D
                return Err(anyhow::anyhow!("aborted"));
            }
            // In raw mode, the enter key might generate \r or \n, check either.
            if b == b'\n' || b == b'\r' {
                break;
            }
            // Store the byte we read and print it back to the terminal.
            bytes.push(b);
            stdout.write_all(&[b]).expect("stdout write failed");
            // Flushing may not be the most efficient but performance isn't critical here.
            stdout.flush()?;
        }
        // Drop _stdout to restore the terminal to normal mode
        std::mem::drop(stdout);
        // We consumed a newline of some kind but didn't echo it, now print
        // one out so subsequent output is guaranteed to be on a new line.
        println!("");
        print!("{}", color::Fg(color::Reset));

        tracing::debug!("exited raw mode and returned to cooked mode");

        let line = String::from_utf8(bytes)?;
        tracing::debug!(?line, "read response line");

        Ok(line)
    }

    async fn get_password(&self) -> Result<String> {
        read_password("Enter Password: ").await
    }
}

impl ActualTerminal {
    pub async fn get_confirmed_password() -> Result<String> {
        loop {
            let password = read_password("Enter Password: ").await?;
            let confirmed = read_password("Confirm Password: ").await?;
            if password != confirmed {
                println!("Password mismatch, please try again.");
                continue;
            }
            return Ok(password);
        }
    }
}
