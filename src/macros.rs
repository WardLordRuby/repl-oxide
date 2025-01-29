/// Convenience macro for calling [`conditionally_remove_hook`](crate::line::LineReader::conditionally_remove_hook)
///
/// **Required Feature** = "macros"  
/// The macros feature flag is included in both "runner" and "background-runner" feature flags
///
/// Internally uses tracing's [`error`](https://docs.rs/tracing/latest/tracing/macro.error.html) to log any errors.
/// Supports both [`Callback`](crate::line::Callback) and [`AsyncCallback`](crate::line::AsyncCallback).
/// `AsyncCallback` branch is accessed by prefixing inputs with the token 'a_sync'
#[macro_export]
macro_rules! process_callback {
    ($line:expr, $callback:expr, $ctx:expr) => {
        if let Err(err) = $callback($ctx) {
            tracing::error!("{err}");
            if let Some(on_err_callback) = $line.conditionally_remove_hook(&err) {
                on_err_callback($ctx).unwrap_or_else(|err| tracing::error!("{err}"))
            }
        }
    };
    (a_sync, $line:expr, $callback:expr, $ctx:expr) => {
        if let Err(err) = $callback($ctx).await {
            tracing::error!("{err}");
            if let Some(on_err_callback) = $line.conditionally_remove_hook(&err) {
                on_err_callback($ctx).unwrap_or_else(|err| tracing::error!("{err}"))
            }
        }
    };
}

/// Convenience macro for the generalized process of handling a streamed [`Event`](https://docs.rs/crossterm/latest/crossterm/event/enum.Event.html)
///
/// **Required Feature** = "macros"  
/// The macros feature flag is included in both "runner" and "background-runner" feature flags
///
/// Internally uses the [`process_callback`] macro that relies on tracing's [`error`](https://docs.rs/tracing/latest/tracing/macro.error.html)
/// to log any errors.
#[macro_export]
macro_rules! general_event_process {
    ($handle:expr, $ctx:expr, $event_result:expr) => {
        match $handle.process_input_event($event_result?)? {
            $crate::EventLoop::Continue => (),
            $crate::EventLoop::Break => break,
            $crate::EventLoop::Callback(callback) => {
                $crate::process_callback!($handle, callback, $ctx)
            }
            $crate::EventLoop::AsyncCallback(callback) => {
                $crate::process_callback!(a_sync, $handle, callback, $ctx)
            }
            $crate::EventLoop::TryProcessInput(Ok(user_tokens)) => {
                match $ctx.try_execute_command(user_tokens).await? {
                    $crate::CommandHandle::Processed => (),
                    $crate::CommandHandle::InsertHook(input_hook) => {
                        $handle.register_input_hook(input_hook)
                    }
                    $crate::CommandHandle::Exit => break,
                }
            }
            $crate::EventLoop::TryProcessInput(Err(mismatched_quotes)) => {
                eprintln!(
                    "{}{mismatched_quotes}{}",
                    $crate::ansi_code::RED,
                    $crate::ansi_code::WHITE
                )
            }
        }
    };
}
