use crate::{executor::Executor, general_event_process, line::Repl};

use std::io::{self, Write};

use crossterm::event::EventStream;
use tokio_stream::StreamExt;

impl<Ctx: Executor<W>, W: Write + Send> Repl<Ctx, W> {
    /// Intended to consume the main function during repl operation. If you are looking to spawn the
    /// repl to be managed by a tokio runtime, see: [`spawn`] using the feature flag "spawner"
    ///
    /// [`spawn`]: crate::line::Repl::spawn
    pub async fn run(&mut self, ctx: &mut Ctx) -> io::Result<()> {
        let mut reader = EventStream::new();

        loop {
            self.clear_unwanted_inputs(&mut reader).await?;
            self.render(ctx)?;

            if let Some(event_result) = reader.next().await {
                general_event_process!(self, ctx, event_result)
            }
        }

        Ok(())
    }
}
