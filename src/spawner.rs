use crate::{executor::Executor, general_event_process, line::LineReader};
use crossterm::event::EventStream;
use std::{
    fmt::Display,
    io::{self, Write},
};
use tokio::{sync::mpsc::Sender, task::JoinHandle};
use tokio_stream::StreamExt;

impl<Ctx, W> LineReader<Ctx, W>
where
    Ctx: Executor<W> + Send + 'static,
    W: Write + Send + 'static,
{
    // MARK: TODO
    // create example for writing your own repl without macros

    /// **Required Feature** = "spawner"
    ///
    /// Spawns the repl, returning you a [`tokio::sync::mpsc::Sender`] as a handle to your terminal output
    /// stream. You must use this channel anytime you need to display background messages to the terminal.
    ///
    /// Generally for advanced cases it is recomended to write your own read eval print loop over an
    /// [`EventStream`] this way will allow for deeper customization, make it easier to spot potential
    /// dead locks, and have all the same functionality `spawn` provides.
    /// See: 'examples/basic_custom.rs' / <WITHOUT_MACRO>
    ///
    /// Avoid using `Ctx`'s whos fields contain `Arc<std::sync::Mutex<T>>` as it would be possible to run
    /// into dead locks if the repl thread tries to access the mutex at the same time as your own main
    /// thread. Using an async aware [`tokio::sync::Mutex`] should avoid dead lock scenarios
    ///
    /// [`EventStream`]: <https://docs.rs/crossterm/0.28.1/crossterm/event/struct.EventStream.html>
    /// [`tokio::sync::Mutex`]: <https://docs.rs/tokio/latest/tokio/sync/struct.Mutex.html>
    /// [`tokio::sync::mpsc::Sender`]: <https://docs.rs/tokio/latest/tokio/sync/mpsc/struct.Sender.html>
    pub fn spawn<M>(mut self, mut ctx: Ctx) -> (JoinHandle<io::Result<()>>, Sender<M>)
    where
        M: Display + Send + 'static,
    {
        let (msg_tx, mut msg_rx) = tokio::sync::mpsc::channel(50);

        let repl_handle = tokio::spawn(async move {
            let mut reader = EventStream::new();

            loop {
                self.clear_unwanted_inputs(&mut reader).await?;
                self.render(&mut ctx)?;

                tokio::select! {
                    biased;

                    Some(event_result) = reader.next() => {
                        general_event_process!(self, &mut ctx, event_result)
                    }

                    Some(msg) = msg_rx.recv() => {
                        self.print_background_msg(msg)?
                    }
                }
            }

            Ok(())
        });

        (repl_handle, msg_tx)
    }
}
