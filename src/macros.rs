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
///     repl.render(&mut command_context)?;
///
///     if let Some(event_result) = reader.next().await {
///         general_event_process!(&mut repl, &mut command_context, event_result)
///     }
/// }
/// ```
///
/// This macro requires you to implement [`Executor`] on your `Ctx`.
///
/// Internally uses tracing's [`error!`] to log any errors that a user created [`AsyncCallback`] produces.
/// As well as emitting a [`trace!`] event if the [`InputHook`] is successfully removed after the error.
///
/// This macro internally uses the try operator on an `io::Result<()>`, and contains break points for the
/// run eval process loop. Requiring the outer scope of to have the same signature, and be called from
/// within a loop.
///
/// [`AsyncCallback`]: crate::line::input_hook::AsyncCallback
/// [`Executor`]: crate::executor::Executor
/// [`InputHook`]: crate::line::input_hook::InputHook
/// [`Event`]: <https://docs.rs/crossterm/latest/crossterm/event/enum.Event.html>
/// [`error!`]: <https://docs.rs/tracing/latest/tracing/macro.error.html>
/// [`trace!`]: <https://docs.rs/tracing/latest/tracing/macro.trace.html>
#[macro_export]
macro_rules! general_event_process {
    ($repl:expr, $ctx:expr, $event_result:expr) => {
        match $repl.process_input_event($ctx, $event_result?)? {
            $crate::EventLoop::Continue => (),
            $crate::EventLoop::Break => break,
            $crate::EventLoop::AsyncCallback(callback) => {
                if let Err(err) = callback($repl, $ctx).await {
                    $repl.prep_for_background_msg()?;
                    tracing::error!("{err}");
                    if $repl.remove_current_hook_by_error($ctx, &err)? {
                        tracing::trace!("Input hook removed after async callback error")
                    };
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
