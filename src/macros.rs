/// Convenience macro for the generalized process of handling a streamed [`Event`]
///
/// The macros feature flag is included in both "runner" and "spawner" feature flags.
///
/// # Example
///
/// ```ignore
/// let mut reader = EventStream::new();
///
/// loop {
///     repl.clear_unwanted_inputs(&mut reader).await?;
///     repl.render(ctx)?;
///
///     if let Some(event_result) = reader.next().await {
///         general_event_process!(&mut repl, &mut ctx, event_result)
///     }
/// }
/// ```
///
/// This macro requries you to implement [`Executor`] on your `ctx`.
///
/// Internally uses tracing's [`error!`] to log any errors.
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
    ($repl:expr, $ctx:expr, $event_result:expr) => {
        match $repl.process_input_event($ctx, $event_result?)? {
            $crate::EventLoop::Continue => (),
            $crate::EventLoop::Break => break,
            $crate::EventLoop::AsyncCallback(callback) => {
                if let Err(err) = callback($repl, $ctx).await {
                    tracing::error!("{err}");
                    $repl.conditionally_remove_hook($ctx, &err)?;
                }
            }
            $crate::EventLoop::TryProcessInput(Ok(user_tokens)) => {
                match $ctx.try_execute_command($repl, user_tokens).await? {
                    $crate::executor::CommandHandle::Processed => (),
                    $crate::executor::CommandHandle::InsertHook(input_hook) => {
                        $repl.register_input_hook(input_hook)
                    }
                    $crate::executor::CommandHandle::Exit => break,
                }
            }
            $crate::EventLoop::TryProcessInput(Err(mismatched_quotes)) => {
                $repl.eprintln(mismatched_quotes)?
            }
        }
    };
}
