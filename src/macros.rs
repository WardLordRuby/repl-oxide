/// Convenience macro for calling [`conditionally_remove_hook`]
///
/// **Required Feature** = "macros"  
/// The macros feature flag is included in both "runner" and "spawner" feature flags
///
/// Internally uses tracing's [`error!`] to log any errors.
///
/// This macro internally uses the try operator on an `io::Result<()>`. Requiring the outer scope of to also
/// have the same signature.
///
/// [`AsyncCallback`]: crate::definitions::callback::AsyncCallback
/// [`conditionally_remove_hook`]: crate::line::LineReader::conditionally_remove_hook
/// [`error!`]: <https://docs.rs/tracing/latest/tracing/macro.error.html>
#[macro_export]
macro_rules! process_async_callback {
    ($line:expr, $callback:expr, $ctx:expr) => {
        if let Err(err) = $callback($line, $ctx).await {
            tracing::error!("{err}");
            $line.conditionally_remove_hook($ctx, &err)?;
        }
    };
}

/// Convenience macro for the generalized process of handling a streamed [`Event`]
///
/// **Required Feature** = "macros"  
/// The macros feature flag is included in both "runner" and "spawner" feature flags.
///
/// This macro requries you to implement [`Executor`] on your `ctx`.
///
/// Internally uses the [`process_async_callback`] macro that relies on tracing's [`error!`] to log any errors.
///
/// This macro internally uses the try operator on an `io::Result<()>`, and contains break points for the
/// run eval process loop. Requiring the outer scope of to have the same signature, and be called from
/// within a loop.
///
/// [`Executor`]: crate::executor::Executor
/// [`Event`]: <https://docs.rs/crossterm/latest/crossterm/event/enum.Event.html>
/// [`error!`]: <https://docs.rs/tracing/latest/tracing/macro.error.html>
#[macro_export]
macro_rules! general_event_process {
    ($line:expr, $ctx:expr, $event_result:expr) => {
        match $line.process_input_event($ctx, $event_result?)? {
            $crate::EventLoop::Continue => (),
            $crate::EventLoop::Break => break,
            $crate::EventLoop::AsyncCallback(callback) => {
                $crate::process_async_callback!($line, callback, $ctx)
            }
            $crate::EventLoop::TryProcessInput(Ok(user_tokens)) => {
                match $ctx.try_execute_command($line, user_tokens).await? {
                    $crate::executor::CommandHandle::Processed => (),
                    $crate::executor::CommandHandle::ExecuteAsyncCallback(callback) => {
                        callback($line, $ctx).await.unwrap_or_else(|err| {
                            tracing::error!("{err}");
                        })
                    }
                    $crate::executor::CommandHandle::InsertHook(input_hook) => {
                        $line.register_input_hook(input_hook)
                    }
                    $crate::executor::CommandHandle::Exit => break,
                }
            }
            $crate::EventLoop::TryProcessInput(Err(mismatched_quotes)) => {
                eprintln!(
                    "{}{mismatched_quotes}{}",
                    $crate::ansi_code::RED,
                    $crate::ansi_code::RESET
                )
            }
        }
    };
}
