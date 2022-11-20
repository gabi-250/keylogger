# Keylogger

[![AGPL-3.0 license][agpl-badge]][agpl-url]
[![Build Status][actions-badge]][actions-url]

[agpl-badge]: https://img.shields.io/badge/license-AGPL-purple.svg
[agpl-url]: https://github.com/gabi-250/keylogger/blob/master/LICENSE
[actions-badge]: https://github.com/gabi-250/keylogger/actions/workflows/test.yaml/badge.svg
[actions-url]: https://github.com/gabi-250/keylogger/actions/workflows/test.yaml?query=branch%3Amaster+workflow%3ATests

This crate provides the necessary scaffolding for handling keyboard input events on Linux.

The installed `KeyboardDevice`s can be detected using `find_keyboards`.
`KeyboardDevice` implements `Stream`, where each element is a `KeyEvent`.

# Example

A simple example that prints the captured keystrokes to stdout. Note the
keylogger needs to run with root privileges.

 ```rust
 use futures::{future, StreamExt};
 use keylogger::{find_keyboards, KeyloggerError};

 #[tokio::main]
 async fn main() -> Result<(), KeyloggerError> {
     let keyboards = find_keyboards()?.into_iter().map(|mut k| async move {
         while let Some(events) = k.next().await {
             println!("[{} @ {}]: ev={events:?}", k.name(), k.path().display());
         }
     });

     future::join_all(keyboards).await;

     Ok(())
 }
 ```

# Disclaimer

This is intended for educational purposes only. Developers assume no liability
and are not responsible for any misuse or damage caused by this program.
