use crate::{
    ansi_code::{RED, WHITE},
    executor::*,
    line::{EventLoop, LineReader},
    process_callback,
};
use crossterm::event::EventStream;
use std::io::{self, Write};
use tokio_stream::StreamExt;

impl<Ctx: Executor<W>, W: Write> LineReader<Ctx, W> {
    pub async fn run(&mut self, ctx: &mut Ctx) -> io::Result<()> {
        let mut reader = EventStream::new();

        loop {
            self.clear_unwanted_inputs(&mut reader).await?;
            self.render()?;

            if let Some(event_result) = reader.next().await {
                match self.process_input_event(event_result?)? {
                    EventLoop::Continue => (),
                    EventLoop::Break => break,
                    EventLoop::Callback(callback) => {
                        process_callback!(self, callback, ctx)
                    }
                    EventLoop::AsyncCallback(callback) => {
                        process_callback!(a_sync, self, callback, ctx)
                    }
                    EventLoop::TryProcessInput(Ok(user_tokens)) => {
                        match ctx.try_execute_command(user_tokens).await? {
                            CommandHandle::Processed => (),
                            CommandHandle::InsertHook(input_hook) => {
                                self.register_input_hook(input_hook)
                            }
                            CommandHandle::Exit => break,
                        }
                    }
                    EventLoop::TryProcessInput(Err(mismatched_quotes)) => {
                        eprintln!("{RED}{mismatched_quotes}{WHITE}")
                    }
                }
            }
        }
        Ok(())
    }
}
