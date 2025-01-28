#[macro_export]
macro_rules! general_event_process {
    ($handle:expr, $ctx:expr, $event_result:expr) => {
        match $handle.process_input_event($event_result?)? {
            EventLoop::Continue => (),
            EventLoop::Break => break,
            EventLoop::Callback(callback) => {
                $crate::process_callback!($handle, callback, $ctx)
            }
            EventLoop::AsyncCallback(callback) => {
                $crate::process_callback!(a_sync, $handle, callback, $ctx)
            }
            EventLoop::TryProcessInput(Ok(user_tokens)) => {
                match $ctx.try_execute_command(user_tokens).await? {
                    CommandHandle::Processed => (),
                    CommandHandle::InsertHook(input_hook) => {
                        $handle.register_input_hook(input_hook)
                    }
                    CommandHandle::Exit => break,
                }
            }
            EventLoop::TryProcessInput(Err(mismatched_quotes)) => {
                eprintln!("{RED}{mismatched_quotes}{WHITE}")
            }
        }
    };
}
