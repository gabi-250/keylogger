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
