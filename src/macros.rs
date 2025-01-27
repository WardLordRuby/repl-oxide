/// Uses tracing's [`error`](https://docs.rs/tracing/latest/tracing/macro.error.html) to log the error
/// then breaks
#[macro_export]
macro_rules! break_if_err {
    ($expr:expr) => {
        if let Err(err) = $expr {
            tracing::error!("{err}");
            break;
        }
    };
}

/// Convenience macro for calling [`conditionally_remove_hook`](crate::line::LineReader::conditionally_remove_hook)
/// uses tracing's [`error`](https://docs.rs/tracing/latest/tracing/macro.error.html) to log any errors
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

/// Matches the input expression and unwraps on `Ok` or uses tracing's
/// [`error`](https://docs.rs/tracing/latest/tracing/macro.error.html) to log the error then breaks
#[macro_export]
macro_rules! unwrap_or_break {
    ($expr:expr) => {
        match $expr {
            Ok(data) => data,
            Err(err) => {
                tracing::error!("{err}");
                break;
            }
        }
    };
}
