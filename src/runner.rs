use crate::{executor::Executor, general_event_process, line::LineReader};
use crossterm::event::EventStream;
use std::io::{self, Write};
use tokio_stream::StreamExt;

impl<Ctx: Executor<W>, W: Write> LineReader<Ctx, W> {
    /// **Required Feature** = "runner"
    ///
    /// Intended to consume the main function during repl operation. If you are looking to run
    /// the repl in the background see: [`background_run`] using the feature flag "background-runner"
    ///
    /// [`background_run`]: crate::line::LineReader::background_run
    pub async fn run(&mut self, ctx: &mut Ctx) -> io::Result<()> {
        let mut reader = EventStream::new();

        loop {
            self.clear_unwanted_inputs(&mut reader).await?;
            self.render()?;

            if let Some(event_result) = reader.next().await {
                general_event_process!(self, ctx, event_result)
            }
        }

        Ok(())
    }
}
