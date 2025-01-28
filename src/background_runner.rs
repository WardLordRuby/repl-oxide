use crate::{
    ansi_code::{RED, WHITE},
    executor::*,
    general_event_process,
    line::{EventLoop, LineReader},
};
use crossterm::event::EventStream;
use std::{
    fmt::Display,
    io::{self, Write},
};
use tokio::sync::mpsc::Sender;
use tokio_stream::StreamExt;

struct SendablePtr<T>(*mut T);

// Safety:
// We are responsible for ensuring that `ptr` is only dereferenced on the thread it is sent to.
unsafe impl<T> Send for SendablePtr<T> {}

impl<T> SendablePtr<T> {
    fn new(from: T) -> Self {
        Self(Box::into_raw(Box::new(from)))
    }
    unsafe fn into_box(self) -> Box<T> {
        Box::from_raw(self.0)
    }
}

/// Convience function to flatten the Os thread handle that is returned from
/// [`background_run`](crate::line::LineReader::background_run)
pub async fn flatten_join(
    handle: std::thread::JoinHandle<tokio::task::JoinHandle<io::Result<()>>>,
) -> io::Result<()> {
    let inner_handle = handle
        .join()
        .map_err(|_| io::Error::other("Thread panicked while running task"))?;

    inner_handle.await?
}

impl<Ctx, W> LineReader<Ctx, W>
where
    Ctx: Executor<W> + 'static,
    W: Write + 'static,
{
    /// Spawns a dedicated OS thread to handle the repl, returning you a `tokio::sync::mpsc::Sender` as
    /// a handle to your terminal output stream. You must use this channel anytime you need to display
    /// background messages to the terminal.
    pub fn background_run<M>(
        self,
        ctx: Ctx,
    ) -> (
        std::thread::JoinHandle<tokio::task::JoinHandle<io::Result<()>>>,
        Sender<M>,
    )
    where
        M: Display + Send + 'static,
    {
        let (msg_tx, mut msg_rx) = tokio::sync::mpsc::channel(50);

        let sendable_line = SendablePtr::new(self);
        let sendable_ctx = SendablePtr::new(ctx);

        let repl_handle = std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();

            let local = tokio::task::LocalSet::new();

            // Saftey:
            //   1. We force users to hand over ownership of the underlying types
            //   2. We only dereference these raw pointers within a `new_current_thread` runtime
            let (mut line_reader, mut command_ctx) =
                unsafe { (sendable_line.into_box(), sendable_ctx.into_box()) };

            let res = local.spawn_local(async move {
                let mut reader = EventStream::new();

                loop {
                    line_reader.clear_unwanted_inputs(&mut reader).await?;
                    line_reader.render()?;

                    tokio::select! {
                        biased;

                        Some(event_result) = reader.next() => {
                            general_event_process!(line_reader, &mut command_ctx, event_result)
                        }

                        Some(msg) = msg_rx.recv() => {
                            line_reader.print_background_msg(msg)?
                        }
                    }
                }
                Ok(())
            });

            rt.block_on(local);
            res
        });

        (repl_handle, msg_tx)
    }
}
