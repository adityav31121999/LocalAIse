use std::net::TcpListener;

use crate::error::{AppError, AppResult};

/// Asks the OS for a free loopback port.
///
/// The listener closes before llama-server binds, so there is a tiny race; the
/// server fails loudly if it loses, and the supervisor reports that to the user.
pub fn pick_free_port() -> AppResult<u16> {
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .map_err(|error| AppError::msg(format!("cannot reserve a local port: {error}")))?;

    let port = listener
        .local_addr()
        .map_err(|error| AppError::msg(format!("cannot read the reserved port: {error}")))?
        .port();

    drop(listener);
    Ok(port)
}
